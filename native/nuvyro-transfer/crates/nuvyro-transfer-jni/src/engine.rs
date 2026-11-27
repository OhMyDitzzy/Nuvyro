use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::command::Command;
use crate::contract::{
    MAX_EVENT_BATCH_BYTES, MAX_EVENTS_PER_POLL, MAX_POLL_TIMEOUT_MS, NativeStatus,
    TERMINAL_EVENT_CAPACITY,
};
use crate::events;

#[derive(Clone)]
struct Session {
    id: String,
    generation: u64,
}

struct State {
    closed: bool,
    generation: u64,
    next_event: u64,
    session: Option<Session>,
    events: VecDeque<Vec<u8>>,
}

pub(crate) struct Engine {
    state: Mutex<State>,
    changed: Condvar,
    polling: AtomicBool,
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
            state: Mutex::new(State {
                closed: false,
                generation: 0,
                next_event: 1,
                session: None,
                events: VecDeque::with_capacity(TERMINAL_EVENT_CAPACITY),
            }),
            changed: Condvar::new(),
            polling: AtomicBool::new(false),
        }
    }

    pub(crate) fn submit(&self, command: Command) -> Result<(), NativeStatus> {
        // Generate outside the state lock so shutdown never waits for OS entropy.
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
        if state.closed {
            return Err(NativeStatus::InvalidHandle);
        }
        match command {
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
                let session = Session {
                    id: new_id.ok_or(NativeStatus::InternalFailure)?,
                    generation,
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
                state.session = None;
                state.generation = state.generation.saturating_add(1).min(i64::MAX as u64);
            }
        }
        self.changed.notify_all();
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
        state.session = None;
        state.events.clear();
        self.changed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
