use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use nuvyro_transfer_core::protocol::Id;

use crate::transport::ConnectCancellation;

const MAX_PAIRING_WORKERS: usize = 2;
const WORKER_POLL: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkerError {
    StartFailed,
    ListenerStopped,
    StaleContext,
}

pub(crate) trait ReceiverPairer<A, P>: Clone + Send + Sync + 'static
where
    A: Send + 'static,
    P: Send + 'static,
{
    fn pair(&self, accepted: A, cancellation: &ConnectCancellation) -> Option<(u64, P)>;
}

pub(crate) trait ReceiverRuntime: Send + 'static {
    type Accepted: Send + 'static;
    type Paired: Send + 'static;
    type Pairer: ReceiverPairer<Self::Accepted, Self::Paired>;

    fn accept(
        &mut self,
        cancellation: &ConnectCancellation,
    ) -> Result<Option<Self::Accepted>, WorkerError>;
    fn pairer(&self) -> Self::Pairer;
    fn bound_route_generation(&self) -> u64 {
        0
    }
    fn close(&mut self);
}

struct ValidityState {
    active: bool,
    paired: bool,
    finished: bool,
    routes: Option<crate::transport::ReceiverRoutes>,
}

/// A pairing result belongs to the exact native session that accepted its socket.
pub(crate) struct SessionValidity {
    generation: u64,
    session_id: String,
    secure_session: Id,
    state: Mutex<ValidityState>,
    finished: Condvar,
}

impl SessionValidity {
    pub(crate) fn new(generation: u64, session_id: &str, secure_session: Id) -> Self {
        Self {
            generation,
            session_id: session_id.to_owned(),
            secure_session,
            state: Mutex::new(ValidityState {
                active: true,
                paired: false,
                finished: false,
                routes: None,
            }),
            finished: Condvar::new(),
        }
    }

    pub(crate) fn matches(&self, generation: u64, session_id: &str) -> bool {
        self.generation == generation && self.session_id == session_id
    }

    pub(crate) fn is_active(&self) -> bool {
        self.state.lock().is_ok_and(|state| state.active)
    }

    fn retain_pair_if_active(&self, route_generation: u64) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        let current_route_generation = state
            .routes
            .as_ref()
            .map_or(0, |routes| routes.context().route_generation());
        if !state.active || state.paired || current_route_generation != route_generation {
            return false;
        }
        state.paired = true;
        true
    }

    pub(crate) fn current_routes(&self) -> Option<crate::transport::ReceiverRoutes> {
        let state = self.state.lock().ok()?;
        state.active.then(|| state.routes.clone()).flatten()
    }

    fn route_generation_matches(&self, generation: u64) -> bool {
        self.state.lock().is_ok_and(|state| {
            state.active
                && state
                    .routes
                    .as_ref()
                    .map_or(0, |routes| routes.context().route_generation())
                    == generation
        })
    }

    pub(crate) fn set_initial_routes(
        &self,
        routes: crate::transport::ReceiverRoutes,
    ) -> Result<(), WorkerError> {
        let mut state = self.state.lock().map_err(|_| WorkerError::StartFailed)?;
        if !state.active
            || routes.context().native_generation() != self.generation
            || routes.context().session() != self.secure_session
        {
            return Err(WorkerError::StartFailed);
        }
        if let Some(current) = state.routes.as_ref() {
            return if current == &routes {
                Ok(())
            } else {
                Err(WorkerError::StartFailed)
            };
        }
        state.routes = Some(routes);
        Ok(())
    }

    pub(crate) fn update_routes(
        &self,
        routes: crate::transport::ReceiverRoutes,
    ) -> Result<(), WorkerError> {
        let mut state = self.state.lock().map_err(|_| WorkerError::StartFailed)?;
        if !state.active {
            return Err(WorkerError::ListenerStopped);
        }
        let current = state.routes.as_ref().ok_or(WorkerError::StartFailed)?;
        if routes == *current {
            return Ok(());
        }
        if routes.context().native_generation() != self.generation
            || routes.context().session() != self.secure_session
            || routes.context().route_generation() <= current.context().route_generation()
        {
            return Err(WorkerError::StaleContext);
        }
        state.routes = Some(routes);
        Ok(())
    }

    pub(crate) fn invalidate(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.active = false;
        }
    }

    fn finish(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.finished = true;
            self.finished.notify_all();
        }
    }

    #[cfg(test)]
    pub(super) fn wait_finished(&self, timeout: Duration) -> bool {
        let Ok(state) = self.state.lock() else {
            return false;
        };
        self.finished
            .wait_timeout_while(state, timeout, |state| !state.finished)
            .is_ok_and(|(state, result)| state.finished && !result.timed_out())
    }

    #[cfg(test)]
    pub(super) fn paired(&self) -> bool {
        self.state.lock().is_ok_and(|state| state.paired)
    }
}

struct JoinEnvelope {
    worker: JoinHandle<()>,
    completion: Arc<(Mutex<bool>, Condvar)>,
}

fn join_queue() -> Option<&'static Sender<JoinEnvelope>> {
    static QUEUE: OnceLock<Option<Sender<JoinEnvelope>>> = OnceLock::new();
    QUEUE
        .get_or_init(|| {
            let (sender, receiver) = mpsc::channel::<JoinEnvelope>();
            thread::Builder::new()
                .name("nuvyro-receiver-join".to_owned())
                .spawn(move || {
                    while let Ok(envelope) = receiver.recv() {
                        let _ = envelope.worker.join();
                        if let Ok(mut complete) = envelope.completion.0.lock() {
                            *complete = true;
                            envelope.completion.1.notify_all();
                        }
                    }
                })
                .ok()?;
            Some(sender)
        })
        .as_ref()
}

pub(crate) struct ReceiverWorkerHandle {
    cancellation: ConnectCancellation,
    validity: Arc<SessionValidity>,
    worker: Option<JoinHandle<()>>,
}

pub(crate) struct StopTicket(Arc<(Mutex<bool>, Condvar)>);

impl StopTicket {
    #[cfg(test)]
    fn wait(&self, timeout: Duration) -> bool {
        let Ok(complete) = self.0.0.lock() else {
            return false;
        };
        self.0
            .1
            .wait_timeout_while(complete, timeout, |complete| !*complete)
            .is_ok_and(|(complete, result)| *complete && !result.timed_out())
    }
}

impl ReceiverWorkerHandle {
    pub(crate) fn spawn<R, F>(
        validity: Arc<SessionValidity>,
        factory: F,
    ) -> Result<Self, WorkerError>
    where
        R: ReceiverRuntime,
        F: FnOnce(ConnectCancellation) -> Result<R, WorkerError> + Send + 'static,
    {
        join_queue().ok_or(WorkerError::StartFailed)?;
        let cancellation = ConnectCancellation::new();
        let worker_cancellation = cancellation.clone();
        let worker_validity = Arc::clone(&validity);
        let worker = thread::Builder::new()
            .name("nuvyro-receiver-session".to_owned())
            .spawn(move || {
                run::<R, F>(factory, worker_cancellation, worker_validity);
            })
            .map_err(|_| WorkerError::StartFailed)?;
        Ok(Self {
            cancellation,
            validity,
            worker: Some(worker),
        })
    }

    pub(crate) fn stop(mut self) -> StopTicket {
        self.stop_inner()
    }

    fn stop_inner(&mut self) -> StopTicket {
        self.validity.invalidate();
        self.cancellation.cancel();
        let Some(worker) = self.worker.take() else {
            let completion = Arc::new((Mutex::new(true), Condvar::new()));
            return StopTicket(completion);
        };
        let completion = Arc::new((Mutex::new(false), Condvar::new()));
        let envelope = JoinEnvelope {
            worker,
            completion: Arc::clone(&completion),
        };
        let queued = match join_queue() {
            Some(queue) => queue.send(envelope),
            None => Err(mpsc::SendError(envelope)),
        };
        if let Err(error) = queued {
            let JoinEnvelope { worker, .. } = error.0;
            let fallback_completion = Arc::clone(&completion);
            let _ = thread::Builder::new()
                .name("nuvyro-receiver-join-retry".to_owned())
                .spawn(move || {
                    let _ = worker.join();
                    if let Ok(mut complete) = fallback_completion.0.lock() {
                        *complete = true;
                        fallback_completion.1.notify_all();
                    }
                });
        }
        StopTicket(completion)
    }
}

impl Drop for ReceiverWorkerHandle {
    fn drop(&mut self) {
        let _ = self.stop_inner();
    }
}

fn run<R, F>(factory: F, cancellation: ConnectCancellation, validity: Arc<SessionValidity>)
where
    R: ReceiverRuntime,
    F: FnOnce(ConnectCancellation) -> Result<R, WorkerError>,
{
    let runtime = match factory(cancellation.clone()) {
        Ok(runtime) => runtime,
        Err(_) => {
            cancellation.cancel();
            validity.invalidate();
            validity.finish();
            return;
        }
    };
    let (results, completed) = mpsc::sync_channel::<Option<(u64, R::Paired)>>(MAX_PAIRING_WORKERS);
    let mut cleanup = OwnerCleanup {
        cancellation: cancellation.clone(),
        runtime: Some(runtime),
        workers: Vec::with_capacity(MAX_PAIRING_WORKERS),
        validity,
    };
    let bound_route_generation = cleanup
        .runtime
        .as_ref()
        .expect("runtime remains owned until worker exit")
        .bound_route_generation();
    let mut paired = None;
    while !cancellation.is_cancelled()
        && cleanup
            .validity
            .route_generation_matches(bound_route_generation)
    {
        cleanup.join_finished();
        while let Ok(result) = completed.try_recv() {
            if let Some((route_generation, connection)) = result
                && paired.is_none()
                && cleanup.validity.retain_pair_if_active(route_generation)
            {
                paired = Some(connection);
            }
        }

        if paired.is_some() || cleanup.workers.len() >= MAX_PAIRING_WORKERS {
            match completed.recv_timeout(WORKER_POLL) {
                Ok(Some((route_generation, connection))) => {
                    if paired.is_none() && cleanup.validity.retain_pair_if_active(route_generation)
                    {
                        paired = Some(connection);
                    }
                }
                Ok(None) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            continue;
        }

        let accepted = match cleanup
            .runtime
            .as_mut()
            .expect("runtime remains owned until worker exit")
            .accept(&cancellation)
        {
            Ok(Some(accepted)) => accepted,
            Ok(None) => continue,
            Err(_) => break,
        };
        let pairer = cleanup
            .runtime
            .as_ref()
            .expect("runtime remains owned until worker exit")
            .pairer();
        let child_cancellation = cancellation.clone();
        let sender = results.clone();
        match thread::Builder::new()
            .name("nuvyro-receiver-pair".to_owned())
            .spawn(move || {
                let result = pairer.pair(accepted, &child_cancellation);
                let _ = sender.send(result);
            }) {
            Ok(worker) => cleanup.workers.push(worker),
            Err(_) => break,
        }
    }
    drop(paired);
    drop(results);
}

struct OwnerCleanup<R: ReceiverRuntime> {
    cancellation: ConnectCancellation,
    runtime: Option<R>,
    workers: Vec<JoinHandle<()>>,
    validity: Arc<SessionValidity>,
}

impl<R: ReceiverRuntime> OwnerCleanup<R> {
    fn join_finished(&mut self) {
        let mut index = 0;
        while index < self.workers.len() {
            if self.workers[index].is_finished() {
                let worker = self.workers.swap_remove(index);
                let _ = worker.join();
            } else {
                index += 1;
            }
        }
    }
}

impl<R: ReceiverRuntime> Drop for OwnerCleanup<R> {
    fn drop(&mut self) {
        self.validity.invalidate();
        self.cancellation.cancel();
        if let Some(mut runtime) = self.runtime.take() {
            runtime.close();
        }
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
        self.validity.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairing::ReceiverIdentity;
    use crate::transport::{ReceiverContext, ReceiverRoute, ReceiverRoutes};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Instant;

    #[derive(Default)]
    struct FakeState {
        accepted: AtomicUsize,
        active: AtomicUsize,
        maximum_active: AtomicUsize,
        released: AtomicBool,
        closed: AtomicBool,
        route_generation: AtomicUsize,
    }

    struct FakeRuntime(Arc<FakeState>);

    #[derive(Clone)]
    struct FakePairer(Arc<FakeState>);

    impl ReceiverRuntime for FakeRuntime {
        type Accepted = usize;
        type Paired = usize;
        type Pairer = FakePairer;

        fn accept(
            &mut self,
            cancellation: &ConnectCancellation,
        ) -> Result<Option<Self::Accepted>, WorkerError> {
            if cancellation.is_cancelled() {
                return Err(WorkerError::ListenerStopped);
            }
            let index = self.0.accepted.fetch_add(1, Ordering::AcqRel);
            if index < MAX_PAIRING_WORKERS {
                Ok(Some(index))
            } else {
                std::thread::sleep(Duration::from_millis(2));
                Ok(None)
            }
        }

        fn pairer(&self) -> Self::Pairer {
            FakePairer(Arc::clone(&self.0))
        }

        fn bound_route_generation(&self) -> u64 {
            self.0.route_generation.load(Ordering::Acquire) as u64
        }

        fn close(&mut self) {
            self.0.closed.store(true, Ordering::Release);
        }
    }

    impl ReceiverPairer<usize, usize> for FakePairer {
        fn pair(
            &self,
            accepted: usize,
            cancellation: &ConnectCancellation,
        ) -> Option<(u64, usize)> {
            let active = self.0.active.fetch_add(1, Ordering::AcqRel) + 1;
            self.0.maximum_active.fetch_max(active, Ordering::AcqRel);
            while !self.0.released.load(Ordering::Acquire) && !cancellation.is_cancelled() {
                thread::sleep(Duration::from_millis(2));
            }
            self.0.active.fetch_sub(1, Ordering::AcqRel);
            if cancellation.is_cancelled() {
                None
            } else {
                Some((
                    self.0.route_generation.load(Ordering::Acquire) as u64,
                    accepted,
                ))
            }
        }
    }

    fn wait_until(timeout: Duration, mut predicate: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if predicate() {
                return true;
            }
            thread::sleep(Duration::from_millis(2));
        }
        predicate()
    }

    #[test]
    fn owner_caps_pairing_workers_and_retains_one_authenticated_connection() {
        let fake = Arc::new(FakeState::default());
        let validity = Arc::new(SessionValidity::new(
            1,
            "11".repeat(16).as_str(),
            Id::from_bytes([0x11; 16]).unwrap(),
        ));
        let handle = ReceiverWorkerHandle::spawn(Arc::clone(&validity), {
            let fake = Arc::clone(&fake);
            move |_| Ok::<_, WorkerError>(FakeRuntime(fake))
        })
        .unwrap();
        assert!(wait_until(Duration::from_secs(2), || {
            fake.maximum_active.load(Ordering::Acquire) == MAX_PAIRING_WORKERS
        }));
        assert_eq!(fake.accepted.load(Ordering::Acquire), MAX_PAIRING_WORKERS);

        fake.released.store(true, Ordering::Release);
        assert!(wait_until(Duration::from_secs(2), || validity.paired()));
        thread::sleep(Duration::from_millis(20));
        assert_eq!(fake.accepted.load(Ordering::Acquire), MAX_PAIRING_WORKERS);

        let ticket = handle.stop();
        assert!(ticket.wait(Duration::from_secs(2)));
        assert!(validity.wait_finished(Duration::from_millis(1)));
        assert!(fake.closed.load(Ordering::Acquire));
        assert_eq!(fake.active.load(Ordering::Acquire), 0);
        assert!(!validity.is_active());
    }

    #[test]
    fn pairing_result_is_rejected_after_its_session_is_invalidated() {
        let validity = SessionValidity::new(
            4,
            "22".repeat(16).as_str(),
            Id::from_bytes([0x22; 16]).unwrap(),
        );
        assert!(validity.matches(4, "22".repeat(16).as_str()));
        assert!(!validity.matches(5, "22".repeat(16).as_str()));
        validity.invalidate();
        assert!(!validity.retain_pair_if_active(0));
        assert!(!validity.paired());
    }

    #[test]
    fn route_generation_update_rejects_a_pair_from_the_previous_snapshot() {
        let identity = ReceiverIdentity::create().unwrap();
        let session = identity.session();
        let validity = SessionValidity::new(4, "22".repeat(16).as_str(), session);
        let old_context = ReceiverContext::new(4, 1, *session.as_bytes()).unwrap();
        let old_route = ReceiverRoute::new(&[10, 0, 0, 1], 0, 24).unwrap();
        let old_routes = ReceiverRoutes::hotspot(old_context, 7, &[old_route], &[], &[]).unwrap();
        validity.set_initial_routes(old_routes.clone()).unwrap();

        let current_context = ReceiverContext::new(4, 2, *session.as_bytes()).unwrap();
        let current_route = ReceiverRoute::new(&[10, 0, 1, 1], 0, 24).unwrap();
        let current_routes =
            ReceiverRoutes::hotspot(current_context, 7, &[current_route], &[], &[]).unwrap();
        validity.update_routes(current_routes.clone()).unwrap();

        assert!(!validity.retain_pair_if_active(1));
        assert!(validity.retain_pair_if_active(2));
        assert_eq!(validity.current_routes(), Some(current_routes));
    }

    #[test]
    fn route_update_stops_owner_while_a_paired_connection_is_retained() {
        let identity = ReceiverIdentity::create().unwrap();
        let session = identity.session();
        let validity = Arc::new(SessionValidity::new(3, "33".repeat(16).as_str(), session));
        let first_context = ReceiverContext::new(3, 1, *session.as_bytes()).unwrap();
        let first_route = ReceiverRoute::new(&[10, 1, 0, 1], 0, 24).unwrap();
        validity
            .set_initial_routes(
                ReceiverRoutes::hotspot(first_context, 7, &[first_route], &[], &[]).unwrap(),
            )
            .unwrap();

        let fake = Arc::new(FakeState::default());
        fake.route_generation.store(1, Ordering::Release);
        let handle = ReceiverWorkerHandle::spawn(Arc::clone(&validity), {
            let fake = Arc::clone(&fake);
            move |_| Ok::<_, WorkerError>(FakeRuntime(fake))
        })
        .unwrap();
        assert!(wait_until(Duration::from_secs(2), || {
            fake.maximum_active.load(Ordering::Acquire) == MAX_PAIRING_WORKERS
        }));
        fake.released.store(true, Ordering::Release);
        assert!(wait_until(Duration::from_secs(2), || validity.paired()));

        let next_context = ReceiverContext::new(3, 2, *session.as_bytes()).unwrap();
        let next_route = ReceiverRoute::new(&[10, 1, 1, 1], 0, 24).unwrap();
        validity
            .update_routes(
                ReceiverRoutes::hotspot(next_context, 7, &[next_route], &[], &[]).unwrap(),
            )
            .unwrap();
        assert!(validity.wait_finished(Duration::from_secs(2)));
        assert!(!validity.is_active());
        assert!(fake.closed.load(Ordering::Acquire));

        let ticket = handle.stop();
        assert!(ticket.wait(Duration::from_secs(2)));
    }
}
