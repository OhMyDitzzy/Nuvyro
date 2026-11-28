use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::command::Command;
use crate::contract::{
    MAX_EVENT_BATCH_BYTES, MAX_EVENTS_PER_POLL, MAX_POLL_TIMEOUT_MS, NativeStatus,
    TERMINAL_EVENT_CAPACITY,
};
use crate::events;

#[cfg(any(target_os = "android", test))]
pub(crate) mod receiver_worker;

#[derive(Clone)]
struct Session {
    id: String,
    generation: u64,
    #[cfg(any(target_os = "android", test))]
    receiver_identity: std::sync::Arc<crate::pairing::ReceiverIdentity>,
    #[cfg(any(target_os = "android", test))]
    receiver_validity: std::sync::Arc<receiver_worker::SessionValidity>,
}

struct State {
    closed: bool,
    generation: u64,
    next_event: u64,
    session: Option<Session>,
    events: VecDeque<Vec<u8>>,
}

pub(crate) struct Engine {
    state: Arc<Mutex<State>>,
    changed: Arc<Condvar>,
    polling: AtomicBool,
    #[cfg(any(target_os = "android", test))]
    receiver_worker: Mutex<Option<receiver_worker::ReceiverWorkerHandle>>,
}

struct PollGuard<'a>(&'a AtomicBool);
impl Drop for PollGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl State {
    fn emit(&mut self, kind: &str, request: &str, session: &Session) -> Result<(), NativeStatus> {
        if self.events.len() >= TERMINAL_EVENT_CAPACITY {
            return Err(NativeStatus::QueueFull);
        }
        let next = self
            .next_event
            .checked_add(1)
            .ok_or(NativeStatus::ResourceLimit)?;
        let event = events::lifecycle(
            kind,
            self.next_event,
            request,
            session.generation,
            &session.id,
        )?;
        self.events.push_back(event);
        self.next_event = next;
        Ok(())
    }
}

impl Engine {
    pub(crate) fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                closed: false,
                generation: 0,
                next_event: 1,
                session: None,
                events: VecDeque::with_capacity(TERMINAL_EVENT_CAPACITY),
            })),
            changed: Arc::new(Condvar::new()),
            polling: AtomicBool::new(false),
            #[cfg(any(target_os = "android", test))]
            receiver_worker: Mutex::new(None),
        }
    }

    pub(crate) fn submit(&self, command: Command) -> Result<(), NativeStatus> {
        let command = match command {
            Command::Receiver {
                update,
                generation,
                session_id,
                routes,
            } => {
                #[cfg(target_os = "android")]
                {
                    let routes = routes.routes(generation, &session_id)?;
                    return if update {
                        self.update_receiver_routes(generation, &session_id, routes)
                    } else {
                        self.attach_receiver(generation, &session_id, routes)
                    };
                }
                #[cfg(not(target_os = "android"))]
                {
                    let _ = (update, generation, session_id, routes);
                    return Err(NativeStatus::InvalidState);
                }
            }
            command => command,
        };
        // Generate the session identity before taking the state lock.
        #[cfg(any(target_os = "android", test))]
        let new_identity = if matches!(&command, Command::Start { .. }) {
            Some(std::sync::Arc::new(
                crate::pairing::ReceiverIdentity::create()
                    .map_err(|_| NativeStatus::InternalFailure)?,
            ))
        } else {
            None
        };
        #[cfg(any(target_os = "android", test))]
        let new_id = new_identity.as_ref().map(|identity| {
            identity
                .session()
                .as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        });
        #[cfg(not(any(target_os = "android", test)))]
        let new_id = if matches!(&command, Command::Start { .. }) {
            let mut bytes = [0u8; 16];
            getrandom::fill(&mut bytes).map_err(|_| NativeStatus::InternalFailure)?;
            if bytes == [0; 16] {
                return Err(NativeStatus::InternalFailure);
            }
            Some(
                bytes
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>(),
            )
        } else {
            None
        };
        let mut state = self
            .state
            .lock()
            .map_err(|_| NativeStatus::InternalFailure)?;
        #[cfg(any(target_os = "android", test))]
        let mut stopped_worker = None;
        if state.closed {
            return Err(NativeStatus::InvalidHandle);
        }
        match command {
            Command::Receiver { .. } => {
                unreachable!("receiver commands dispatched before lifecycle lock")
            }
            Command::Start { request_id } => {
                if state.session.is_some() {
                    return Err(NativeStatus::InvalidState);
                }
                // Reserve one slot now for the matching SessionClosed event.
                if state.events.len() + 2 > TERMINAL_EVENT_CAPACITY {
                    return Err(NativeStatus::QueueFull);
                }
                let generation = state
                    .generation
                    .checked_add(1)
                    .filter(|v| *v <= i64::MAX as u64)
                    .ok_or(NativeStatus::ResourceLimit)?;
                let id = new_id.ok_or(NativeStatus::InternalFailure)?;
                #[cfg(any(target_os = "android", test))]
                let receiver_identity = new_identity.ok_or(NativeStatus::InternalFailure)?;
                #[cfg(any(target_os = "android", test))]
                let receiver_validity = std::sync::Arc::new(receiver_worker::SessionValidity::new(
                    generation,
                    &id,
                    receiver_identity.session(),
                ));
                let session = Session {
                    id,
                    generation,
                    #[cfg(any(target_os = "android", test))]
                    receiver_identity,
                    #[cfg(any(target_os = "android", test))]
                    receiver_validity,
                };
                state.emit("SessionStarted", &request_id, &session)?;
                state.generation = generation;
                state.session = Some(session);
            }
            Command::Close {
                request_id,
                generation,
                session_id,
            } => {
                let session = state.session.clone().ok_or(NativeStatus::InvalidState)?;
                if session.generation != generation || session.id != session_id {
                    return Err(NativeStatus::InvalidState);
                }
                state.emit("SessionClosed", &request_id, &session)?;
                #[cfg(any(target_os = "android", test))]
                {
                    session.receiver_validity.invalidate();
                    session.receiver_identity.close();
                }
                state.session = None;
                state.generation = state.generation.saturating_add(1).min(i64::MAX as u64);
                #[cfg(any(target_os = "android", test))]
                {
                    stopped_worker = self
                        .receiver_worker
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .take();
                }
            }
        }
        self.changed.notify_all();
        drop(state);
        #[cfg(any(target_os = "android", test))]
        if let Some(worker) = stopped_worker {
            worker.stop();
        }
        Ok(())
    }

    #[cfg(target_os = "android")]
    pub(crate) fn attach_receiver(
        &self,
        generation: u64,
        session_id: &str,
        routes: crate::transport::ReceiverRoutes,
    ) -> Result<(), NativeStatus> {
        let (identity, validity) = {
            let state = self
                .state
                .lock()
                .map_err(|_| NativeStatus::InternalFailure)?;
            if state.closed {
                return Err(NativeStatus::InvalidHandle);
            }
            let session = state.session.as_ref().ok_or(NativeStatus::InvalidState)?;
            if session.generation != generation || session.id != session_id {
                return Err(NativeStatus::InvalidState);
            }
            if routes.context().native_generation() != generation
                || routes.context().session() != session.receiver_identity.session()
            {
                return Err(NativeStatus::InvalidState);
            }
            session
                .receiver_validity
                .set_initial_routes(routes)
                .map_err(|_| NativeStatus::InvalidState)?;
            (
                std::sync::Arc::clone(&session.receiver_identity),
                std::sync::Arc::clone(&session.receiver_validity),
            )
        };
        self.attach_receiver_with(generation, session_id, move |cancellation| {
            crate::transport::receiver_runtime(identity, validity, cancellation)
        })
    }

    #[cfg(target_os = "android")]
    pub(crate) fn update_receiver_routes(
        &self,
        generation: u64,
        session_id: &str,
        routes: crate::transport::ReceiverRoutes,
    ) -> Result<(), NativeStatus> {
        let state = self
            .state
            .lock()
            .map_err(|_| NativeStatus::InternalFailure)?;
        if state.closed {
            return Err(NativeStatus::InvalidHandle);
        }
        let session = state.session.as_ref().ok_or(NativeStatus::InvalidState)?;
        if session.generation != generation
            || session.id != session_id
            || routes.context().native_generation() != generation
            || routes.context().session() != session.receiver_identity.session()
        {
            return Err(NativeStatus::InvalidState);
        }
        let worker = self
            .receiver_worker
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if worker.is_none() {
            return Err(NativeStatus::InvalidState);
        }
        session
            .receiver_validity
            .update_routes(routes)
            .map_err(|_| NativeStatus::InvalidState)
    }

    #[cfg(any(target_os = "android", test))]
    fn attach_receiver_with<R, F>(
        &self,
        generation: u64,
        session_id: &str,
        factory: F,
    ) -> Result<(), NativeStatus>
    where
        R: receiver_worker::ReceiverRuntime,
        F: FnOnce(crate::transport::ConnectCancellation) -> Result<R, receiver_worker::WorkerError>
            + Send
            + 'static,
    {
        let state = self
            .state
            .lock()
            .map_err(|_| NativeStatus::InternalFailure)?;
        if state.closed {
            return Err(NativeStatus::InvalidHandle);
        }
        let session = state.session.as_ref().ok_or(NativeStatus::InvalidState)?;
        if session.generation != generation
            || session.id != session_id
            || !session.receiver_validity.matches(generation, session_id)
        {
            return Err(NativeStatus::InvalidState);
        }
        let mut worker = self
            .receiver_worker
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if worker.is_some() || !session.receiver_validity.is_active() {
            return Err(NativeStatus::InvalidState);
        }
        if state.events.len() + 3 > TERMINAL_EVENT_CAPACITY
            || state.next_event.checked_add(3).is_none()
        {
            return Err(NativeStatus::QueueFull);
        }
        let events = Arc::clone(&self.state);
        let changed = Arc::clone(&self.changed);
        let expected = session.clone();
        let route_generation = session
            .receiver_validity
            .current_routes()
            .map_or(0, |routes| routes.context().route_generation());
        *worker = Some(
            receiver_worker::ReceiverWorkerHandle::spawn_observed(
                std::sync::Arc::clone(&session.receiver_validity),
                factory,
                move |listening| {
                    // A closed or replaced session cannot publish a late worker event.
                    let Ok(mut state) = events.lock() else {
                        return;
                    };
                    if state.closed
                        || !state.session.as_ref().is_some_and(|current| {
                            current.id == expected.id && current.generation == expected.generation
                        })
                    {
                        return;
                    }
                    if listening
                        && (!expected.receiver_validity.is_active()
                            || expected
                                .receiver_validity
                                .current_routes()
                                .map_or(0, |routes| routes.context().route_generation())
                                != route_generation)
                    {
                        return;
                    }
                    let kind = if listening {
                        "ReceiverListening"
                    } else {
                        "ReceiverStopped"
                    };
                    if let Ok(event) = events::receiver(
                        kind,
                        state.next_event,
                        expected.generation,
                        &expected.id,
                        route_generation,
                    ) {
                        state.events.push_back(event);
                        state.next_event += 1;
                        changed.notify_all();
                    }
                },
            )
            .map_err(|_| NativeStatus::ResourceLimit)?,
        );
        Ok(())
    }

    pub(crate) fn poll_with<T>(
        &self,
        maximum: usize,
        timeout_ms: i32,
        deliver: impl FnOnce(&[u8]) -> Result<T, NativeStatus>,
    ) -> Result<T, NativeStatus> {
        if maximum == 0
            || maximum > MAX_EVENTS_PER_POLL
            || !(0..=MAX_POLL_TIMEOUT_MS).contains(&timeout_ms)
        {
            return Err(NativeStatus::InvalidArgument);
        }
        self.polling
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| NativeStatus::InvalidState)?;
        let _poll = PollGuard(&self.polling);
        let deadline = Instant::now() + Duration::from_millis(timeout_ms as u64);
        let mut state = self
            .state
            .lock()
            .map_err(|_| NativeStatus::InternalFailure)?;
        while state.events.is_empty() && !state.closed && Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            state = self
                .changed
                .wait_timeout(state, remaining)
                .map_err(|_| NativeStatus::InternalFailure)?
                .0;
        }
        if state.closed {
            return Err(NativeStatus::InvalidHandle);
        }
        let mut bytes = vec![b'['];
        let mut count = 0;
        for event in state.events.iter().take(maximum) {
            if bytes.len() + usize::from(count != 0) + event.len() + 1 > MAX_EVENT_BATCH_BYTES {
                break;
            }
            if count != 0 {
                bytes.push(b',');
            }
            bytes.extend_from_slice(event);
            count += 1;
        }
        bytes.push(b']');
        // JNI must create the Java array successfully before terminal events leave this queue.
        let result = deliver(&bytes)?;
        state.events.drain(..count);
        Ok(result)
    }

    pub(crate) fn close(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.closed = true;
        #[cfg(any(target_os = "android", test))]
        if let Some(session) = state.session.as_ref() {
            session.receiver_validity.invalidate();
            session.receiver_identity.close();
        }
        state.session = None;
        state.events.clear();
        self.changed.notify_all();
        drop(state);
        #[cfg(any(target_os = "android", test))]
        if let Some(worker) = self
            .receiver_worker
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
        {
            worker.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::receiver_worker::{ReceiverPairer, ReceiverRuntime, WorkerError};
    use crate::transport::ConnectCancellation;
    use std::sync::atomic::{AtomicBool, Ordering};
    fn start(engine: &Engine) {
        engine
            .submit(Command::Start {
                request_id: "11".repeat(16),
            })
            .unwrap();
    }
    fn close_session(engine: &Engine) {
        let session = engine.state.lock().unwrap().session.clone().unwrap();
        engine
            .submit(Command::Close {
                request_id: "22".repeat(16),
                generation: session.generation,
                session_id: session.id,
            })
            .unwrap();
    }
    #[test]
    fn reserves_close_capacity_and_keeps_events_on_delivery_failure() {
        let engine = Engine::new();
        for _ in 0..8 {
            start(&engine);
            close_session(&engine);
        }
        assert_eq!(
            engine.submit(Command::Start {
                request_id: "11".repeat(16)
            }),
            Err(NativeStatus::QueueFull)
        );
        assert_eq!(
            engine.poll_with::<()>(1, 0, |_| Err(NativeStatus::InternalFailure)),
            Err(NativeStatus::InternalFailure)
        );
        let batch = engine.poll_with(2, 0, |data| Ok(data.to_vec())).unwrap();
        assert_eq!(
            serde_json::from_slice::<Vec<serde_json::Value>>(&batch)
                .unwrap()
                .len(),
            2
        );
        start(&engine);
        close_session(&engine);
        assert_eq!(engine.state.lock().unwrap().events.len(), 16);
    }
    #[test]
    fn stale_close_does_not_end_new_session() {
        let engine = Engine::new();
        start(&engine);
        let old = engine.state.lock().unwrap().session.clone().unwrap();
        close_session(&engine);
        start(&engine);
        assert_eq!(
            engine.submit(Command::Close {
                request_id: "33".repeat(16),
                generation: old.generation,
                session_id: old.id
            }),
            Err(NativeStatus::InvalidState)
        );
    }

    #[test]
    fn engine_session_id_is_the_receiver_identity_session_id() {
        let engine = Engine::new();
        start(&engine);
        let session = engine.state.lock().unwrap().session.clone().unwrap();
        let secure_id = session
            .receiver_identity
            .session()
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(session.id, secure_id);
    }

    #[test]
    fn shutdown_invalidates_concurrent_poll() {
        let engine = std::sync::Arc::new(Engine::new());
        let peer = engine.clone();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let ready = barrier.clone();
        let thread = std::thread::spawn(move || {
            ready.wait();
            peer.poll_with(1, 1000, |data| Ok(data.to_vec()))
        });
        barrier.wait();
        engine.close();
        assert_eq!(thread.join().unwrap(), Err(NativeStatus::InvalidHandle));
    }

    struct PairControl {
        started: AtomicBool,
        release: AtomicBool,
        closed: AtomicBool,
    }

    #[test]
    fn receiver_startup_failure_emits_stopped_without_listening() {
        let engine = Engine::new();
        start(&engine);
        let session = engine.state.lock().unwrap().session.clone().unwrap();
        engine
            .attach_receiver_with::<PairRuntime, _>(session.generation, &session.id, |_| {
                Err(WorkerError::StartFailed)
            })
            .unwrap();
        assert!(
            session
                .receiver_validity
                .wait_finished(Duration::from_secs(2))
        );
        let mut kinds = Vec::new();
        while kinds.len() < 2 {
            let batch = engine
                .poll_with(4, 1000, |bytes| Ok(bytes.to_vec()))
                .unwrap();
            let events: Vec<serde_json::Value> = serde_json::from_slice(&batch).unwrap();
            assert!(!events.is_empty());
            kinds.extend(
                events
                    .into_iter()
                    .map(|event| event["kind"].as_str().unwrap().to_owned()),
            );
        }
        assert_eq!(kinds, ["SessionStarted", "ReceiverStopped"]);
    }

    #[test]
    fn late_listener_startup_cannot_emit_events_into_replacement_session() {
        let engine = Engine::new();
        start(&engine);
        let session = engine.state.lock().unwrap().session.clone().unwrap();
        let (release, blocked) = std::sync::mpsc::channel();
        let control = Arc::new(PairControl {
            started: AtomicBool::new(false),
            release: AtomicBool::new(true),
            closed: AtomicBool::new(false),
        });
        engine
            .attach_receiver_with(session.generation, &session.id, move |_| {
                blocked.recv().unwrap();
                Ok(PairRuntime(control))
            })
            .unwrap();
        close_session(&engine);
        start(&engine);
        release.send(()).unwrap();
        assert!(
            session
                .receiver_validity
                .wait_finished(Duration::from_secs(2))
        );
        let batch = engine.poll_with(8, 0, |bytes| Ok(bytes.to_vec())).unwrap();
        let events: Vec<serde_json::Value> = serde_json::from_slice(&batch).unwrap();
        assert_eq!(
            events
                .iter()
                .map(|event| event["kind"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["SessionStarted", "SessionClosed", "SessionStarted"]
        );
    }

    #[test]
    fn reserves_worker_terminal_and_close_event_capacity_before_spawn() {
        let engine = Engine::new();
        for _ in 0..7 {
            start(&engine);
            close_session(&engine);
        }
        start(&engine);
        let session = engine.state.lock().unwrap().session.clone().unwrap();
        assert_eq!(
            engine.attach_receiver_with::<PairRuntime, _>(
                session.generation,
                &session.id,
                |_| panic!("must not spawn without event capacity")
            ),
            Err(NativeStatus::QueueFull)
        );
        close_session(&engine);
        assert_eq!(
            engine.state.lock().unwrap().events.len(),
            TERMINAL_EVENT_CAPACITY
        );
    }

    struct IdleRuntime {
        route_generation: u64,
        closed: Arc<AtomicBool>,
    }

    #[derive(Clone)]
    struct IdlePairer;

    impl ReceiverRuntime for IdleRuntime {
        type Accepted = ();
        type Paired = ();
        type Pairer = IdlePairer;

        fn accept(&mut self, _: &ConnectCancellation) -> Result<Option<()>, WorkerError> {
            std::thread::sleep(Duration::from_millis(2));
            Ok(None)
        }
        fn pairer(&self) -> IdlePairer {
            IdlePairer
        }
        fn bound_route_generation(&self) -> u64 {
            self.route_generation
        }
        fn close(&mut self) {
            self.closed.store(true, Ordering::Release);
        }
    }

    impl ReceiverPairer<(), ()> for IdlePairer {
        fn pair(&self, _: (), _: &ConnectCancellation) -> Option<(u64, ())> {
            None
        }
    }

    #[test]
    fn route_change_emits_stopped_for_old_route_after_listener_close() {
        use crate::transport::{ReceiverContext, ReceiverRoute, ReceiverRoutes};
        let engine = Engine::new();
        start(&engine);
        let session = engine.state.lock().unwrap().session.clone().unwrap();
        let old = ReceiverRoutes::wifi(
            ReceiverContext::new(
                session.generation,
                1,
                *session.receiver_identity.session().as_bytes(),
            )
            .unwrap(),
            u64::MAX,
            7,
            &[ReceiverRoute::new(&[192, 168, 42, 1], 0, 24).unwrap()],
        )
        .unwrap();
        let next = ReceiverRoutes::wifi(
            ReceiverContext::new(
                session.generation,
                2,
                *session.receiver_identity.session().as_bytes(),
            )
            .unwrap(),
            u64::MAX,
            7,
            &[ReceiverRoute::new(&[192, 168, 42, 2], 0, 24).unwrap()],
        )
        .unwrap();
        session.receiver_validity.set_initial_routes(old).unwrap();
        let closed = Arc::new(AtomicBool::new(false));
        engine
            .attach_receiver_with::<IdleRuntime, _>(session.generation, &session.id, {
                let closed = Arc::clone(&closed);
                move |_| {
                    Ok(IdleRuntime {
                        route_generation: 1,
                        closed,
                    })
                }
            })
            .unwrap();
        let mut kinds = Vec::new();
        while !kinds.iter().any(|kind| kind == "ReceiverListening") {
            let batch = engine
                .poll_with(4, 1000, |bytes| Ok(bytes.to_vec()))
                .unwrap();
            let events: Vec<serde_json::Value> = serde_json::from_slice(&batch).unwrap();
            kinds.extend(
                events
                    .into_iter()
                    .map(|event| event["kind"].as_str().unwrap().to_owned()),
            );
        }
        engine
            .update_receiver_routes(session.generation, &session.id, next)
            .unwrap();
        assert!(
            session
                .receiver_validity
                .wait_finished(Duration::from_secs(2))
        );
        assert!(closed.load(Ordering::Acquire));
        let batch = engine
            .poll_with(4, 1000, |bytes| Ok(bytes.to_vec()))
            .unwrap();
        let events: Vec<serde_json::Value> = serde_json::from_slice(&batch).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["kind"], "ReceiverStopped");
        assert_eq!(events[0]["data"]["route_generation"], "1");
        assert_eq!(events[0]["data"]["transport_ready"], false);
    }

    struct PairRuntime(std::sync::Arc<PairControl>);

    #[derive(Clone)]
    struct LatePairer(std::sync::Arc<PairControl>);

    impl ReceiverRuntime for PairRuntime {
        type Accepted = ();
        type Paired = ();
        type Pairer = LatePairer;

        fn accept(
            &mut self,
            cancellation: &ConnectCancellation,
        ) -> Result<Option<Self::Accepted>, WorkerError> {
            if cancellation.is_cancelled() {
                return Err(WorkerError::ListenerStopped);
            }
            std::thread::sleep(Duration::from_millis(2));
            Ok(Some(()))
        }

        fn pairer(&self) -> Self::Pairer {
            LatePairer(std::sync::Arc::clone(&self.0))
        }

        fn close(&mut self) {
            self.0.closed.store(true, Ordering::Release);
        }
    }

    impl ReceiverPairer<(), ()> for LatePairer {
        fn pair(&self, _: (), _: &ConnectCancellation) -> Option<(u64, ())> {
            self.0.started.store(true, Ordering::Release);
            while !self.0.release.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(2));
            }
            Some((0, ()))
        }
    }

    #[test]
    fn closing_session_cancels_listener_joins_workers_and_rejects_late_pairing() {
        let engine = Engine::new();
        start(&engine);
        let session = engine.state.lock().unwrap().session.clone().unwrap();
        let validity = std::sync::Arc::clone(&session.receiver_validity);
        let control = std::sync::Arc::new(PairControl {
            started: AtomicBool::new(false),
            release: AtomicBool::new(false),
            closed: AtomicBool::new(false),
        });
        engine
            .attach_receiver_with(session.generation, &session.id, {
                let control = std::sync::Arc::clone(&control);
                move |_| Ok::<_, WorkerError>(PairRuntime(control))
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !control.started.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(control.started.load(Ordering::Acquire));

        close_session(&engine);
        control.release.store(true, Ordering::Release);
        assert!(validity.wait_finished(Duration::from_secs(2)));
        assert!(!validity.paired());
        assert!(!validity.is_active());
        assert!(control.closed.load(Ordering::Acquire));

        let batch = engine
            .poll_with(4, 0, |events| Ok(events.to_vec()))
            .unwrap();
        let events: Vec<serde_json::Value> = serde_json::from_slice(&batch).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0]["kind"], "SessionStarted");
        assert_eq!(events[1]["kind"], "ReceiverListening");
        assert_eq!(events[2]["kind"], "SessionClosed");
    }
}
