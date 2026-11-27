use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

use super::*;
use crate::transport::ConnectCancellation;

mod pairing;

struct Socket(Arc<AtomicUsize>);

impl Drop for Socket {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

fn context() -> ReceiverContext {
    ReceiverContext::new(1, 2, [0x11; 16]).unwrap()
}

fn admit(
    admission: &ReceiverAdmission,
    now: Instant,
    drops: &Arc<AtomicUsize>,
) -> Result<PendingIncoming<Socket>, ReceiverError> {
    admission.admit(
        Socket(Arc::clone(drops)),
        ConnectCancellation::new(),
        || now,
    )
}

#[test]
fn context_rejects_invalid_generations_and_sessions_without_needing_a_network_handle() {
    for (native, route, session) in [
        (0, 1, [1; 16]),
        (1, 0, [1; 16]),
        (u64::MAX, 1, [1; 16]),
        (1, u64::MAX, [1; 16]),
        (1, 1, [0; 16]),
    ] {
        assert_eq!(
            ReceiverContext::new(native, route, session),
            Err(ReceiverError::InvalidContext)
        );
    }
    assert_eq!(context().session().as_bytes(), &[0x11; 16]);
    assert_eq!(format!("{:?}", context()), "ReceiverContext { .. }");
}

#[test]
fn only_two_unpaired_sockets_are_owned_and_drop_releases_a_slot() {
    let now = Instant::now();
    let admission = ReceiverAdmission::new(context(), now);
    let drops = Arc::new(AtomicUsize::new(0));
    let first = admit(&admission, now, &drops).unwrap();
    let second = admit(&admission, now, &drops).unwrap();
    assert!(matches!(
        admit(&admission, now, &drops),
        Err(ReceiverError::Busy)
    ));
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    drop(first);
    let replacement = admit(&admission, now, &drops).unwrap();
    drop((second, replacement));
    assert_eq!(drops.load(Ordering::Relaxed), 4);
}

#[test]
fn dropping_failed_attempts_never_refunds_the_eight_attempt_rolling_limit() {
    let now = Instant::now();
    let admission = ReceiverAdmission::new(context(), now);
    let drops = Arc::new(AtomicUsize::new(0));
    for _ in 0..8 {
        drop(admit(&admission, now, &drops).unwrap());
    }
    assert!(matches!(
        admit(&admission, now, &drops),
        Err(ReceiverError::RateLimited)
    ));
    assert!(matches!(
        admit(&admission, now + Duration::from_millis(59_999), &drops),
        Err(ReceiverError::RateLimited)
    ));
    drop(admit(&admission, now + Duration::from_secs(60), &drops).unwrap());
    assert_eq!(drops.load(Ordering::Relaxed), 11);
}

#[test]
fn rolling_window_expires_each_attempt_instead_of_resetting_a_minute_bucket() {
    let now = Instant::now();
    let admission = ReceiverAdmission::new(context(), now);
    let drops = Arc::new(AtomicUsize::new(0));
    for i in 0..8 {
        drop(admit(&admission, now + Duration::from_secs(i), &drops).unwrap());
    }
    drop(admit(&admission, now + Duration::from_secs(60), &drops).unwrap());
    assert!(matches!(
        admit(&admission, now + Duration::from_secs(60), &drops),
        Err(ReceiverError::RateLimited)
    ));
    drop(admit(&admission, now + Duration::from_secs(61), &drops).unwrap());
}

#[test]
fn capacity_and_pre_cancelled_rejections_do_not_start_or_charge_tls() {
    let now = Instant::now();
    let admission = ReceiverAdmission::new(context(), now);
    let drops = Arc::new(AtomicUsize::new(0));
    let cancellation = ConnectCancellation::new();
    cancellation.cancel();
    assert!(matches!(
        admission.admit(Socket(Arc::clone(&drops)), cancellation, || now),
        Err(ReceiverError::Cancelled)
    ));
    let first = admit(&admission, now, &drops).unwrap();
    let second = admit(&admission, now, &drops).unwrap();
    for _ in 0..10 {
        assert!(matches!(
            admit(&admission, now, &drops),
            Err(ReceiverError::Busy)
        ));
    }
    drop((first, second));
    for _ in 0..6 {
        drop(admit(&admission, now, &drops).unwrap());
    }
    assert!(matches!(
        admit(&admission, now, &drops),
        Err(ReceiverError::RateLimited)
    ));
}

#[test]
fn concurrent_admission_never_exceeds_two_and_keeps_rejected_sockets_out() {
    let now = Instant::now();
    let admission = ReceiverAdmission::new(context(), now);
    let drops = Arc::new(AtomicUsize::new(0));
    let successes = AtomicUsize::new(0);
    let barrier = Barrier::new(17);
    std::thread::scope(|scope| {
        for _ in 0..16 {
            let (admission, drops, successes, barrier) = (&admission, &drops, &successes, &barrier);
            scope.spawn(move || {
                let pending = admit(admission, now, drops);
                match &pending {
                    Ok(_) => {
                        successes.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(error) => assert_eq!(*error, ReceiverError::Busy),
                }
                barrier.wait();
                barrier.wait();
                drop(pending);
            });
        }
        barrier.wait();
        assert_eq!(successes.load(Ordering::Relaxed), 2);
        assert!(matches!(
            admit(&admission, now, &drops),
            Err(ReceiverError::Busy)
        ));
        barrier.wait();
    });
    assert_eq!(drops.load(Ordering::Relaxed), 17);
    drop(admit(&admission, now, &drops).unwrap());
}

#[test]
fn closing_or_dropping_the_admission_owner_invalidates_existing_permits() {
    for explicit in [false, true] {
        let now = Instant::now();
        let admission = ReceiverAdmission::new(context(), now);
        let drops = Arc::new(AtomicUsize::new(0));
        let mut pending = admit(&admission, now, &drops).unwrap();
        if explicit {
            admission.close();
            assert!(matches!(
                admit(&admission, now, &drops),
                Err(ReceiverError::Closed)
            ));
        }
        drop(admission);
        assert_eq!(
            pending.check_context_at(Some(context()), now),
            Err(ReceiverError::Closed)
        );
        assert_eq!(drops.load(Ordering::Relaxed), 1 + usize::from(explicit));
    }
}

#[test]
fn backward_admission_clock_closes_the_owner_instead_of_resetting_the_rate_limit() {
    let now = Instant::now();
    let admission = ReceiverAdmission::new(context(), now);
    let drops = Arc::new(AtomicUsize::new(0));
    let mut pending = admit(&admission, now, &drops).unwrap();
    drop(admit(&admission, now + Duration::from_secs(1), &drops).unwrap());
    assert!(matches!(
        admit(&admission, now, &drops),
        Err(ReceiverError::ClockInvalid)
    ));
    assert_eq!(
        pending.check_context_at(Some(context()), now),
        Err(ReceiverError::Closed)
    );
    assert!(matches!(
        admit(&admission, now + Duration::from_secs(60), &drops),
        Err(ReceiverError::Closed)
    ));
}

#[test]
fn a_pending_socket_rejects_every_changed_generation_session_or_missing_context() {
    for current in [
        None,
        Some(ReceiverContext::new(2, 2, [0x11; 16]).unwrap()),
        Some(ReceiverContext::new(1, 3, [0x11; 16]).unwrap()),
        Some(ReceiverContext::new(1, 2, [0x22; 16]).unwrap()),
    ] {
        let now = Instant::now();
        let admission = ReceiverAdmission::new(context(), now);
        let drops = Arc::new(AtomicUsize::new(0));
        let mut pending = admit(&admission, now, &drops).unwrap();
        assert_eq!(
            pending.check_context_at(current, now),
            Err(ReceiverError::StaleContext)
        );
        assert_eq!(drops.load(Ordering::Relaxed), 1);
        assert_eq!(
            pending.check_context_at(Some(context()), now),
            Err(ReceiverError::Cancelled)
        );
        drop(admit(&admission, now, &drops).unwrap());
    }
}

#[test]
fn cancellation_drops_the_socket_and_the_unpaired_permit() {
    let now = Instant::now();
    let admission = ReceiverAdmission::new(context(), now);
    let drops = Arc::new(AtomicUsize::new(0));
    let cancellation = ConnectCancellation::new();
    let mut pending = admission
        .admit(Socket(Arc::clone(&drops)), cancellation.clone(), || now)
        .unwrap();
    cancellation.cancel();
    assert_eq!(
        pending.check_context_at(Some(context()), now),
        Err(ReceiverError::Cancelled)
    );
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    let _first = admit(&admission, now, &drops).unwrap();
    let _second = admit(&admission, now, &drops).unwrap();
}

#[test]
fn pending_time_and_poll_windows_never_extend_the_original_ten_seconds() {
    let now = Instant::now();
    let admission = ReceiverAdmission::new(context(), now);
    let drops = Arc::new(AtomicUsize::new(0));
    let mut pending = admit(&admission, now, &drops).unwrap();
    assert_eq!(
        pending.poll_window(Some(context()), now).unwrap(),
        Duration::from_millis(50)
    );
    assert_eq!(
        pending
            .poll_window(Some(context()), now + Duration::from_millis(9_999))
            .unwrap(),
        Duration::from_millis(1)
    );
    assert_eq!(
        pending.check_context_at(Some(context()), now + Duration::from_secs(10)),
        Err(ReceiverError::DeadlineExceeded)
    );
    assert_eq!(drops.load(Ordering::Relaxed), 1);
}

#[test]
fn a_backward_observation_after_progress_closes_the_pending_socket() {
    let now = Instant::now();
    let admission = ReceiverAdmission::new(context(), now);
    let drops = Arc::new(AtomicUsize::new(0));
    let mut pending = admit(&admission, now, &drops).unwrap();
    pending
        .check_context_at(Some(context()), now + Duration::from_secs(1))
        .unwrap();
    assert_eq!(
        pending.check_context_at(Some(context()), now),
        Err(ReceiverError::ClockInvalid)
    );
    assert_eq!(drops.load(Ordering::Relaxed), 1);
}
