use super::*;

#[test]
fn only_pair_result_completion_releases_unpaired_and_keeps_one_paired_slot() {
    let now = Instant::now();
    let admission = ReceiverAdmission::new(context(), now);
    let drops = Arc::new(AtomicUsize::new(0));
    let mut first = admit(&admission, now, &drops).unwrap();
    let mut second = admit(&admission, now, &drops).unwrap();
    let reservation = first.reserve_pair(Some(context()), now).unwrap();
    assert!(matches!(
        second.reserve_pair(Some(context()), now),
        Err(ReceiverError::Busy)
    ));
    assert!(matches!(
        admit(&admission, now, &drops),
        Err(ReceiverError::Busy)
    ));
    first
        .promote_pair(reservation, Some(context()), now)
        .unwrap();
    let third = admit(&admission, now, &drops).unwrap();
    assert!(matches!(
        second.reserve_pair(Some(context()), now),
        Err(ReceiverError::Busy)
    ));
    drop(first);
    drop(second.reserve_pair(Some(context()), now).unwrap());
    drop((second, third));
}

#[test]
fn abandoned_pair_reservations_keep_unpaired_ownership_and_release_exclusivity() {
    let now = Instant::now();
    let admission = ReceiverAdmission::new(context(), now);
    let drops = Arc::new(AtomicUsize::new(0));
    let mut first = admit(&admission, now, &drops).unwrap();
    let mut second = admit(&admission, now, &drops).unwrap();
    drop(first.reserve_pair(Some(context()), now).unwrap());
    assert!(matches!(
        admit(&admission, now, &drops),
        Err(ReceiverError::Busy)
    ));
    drop(second.reserve_pair(Some(context()), now).unwrap());
}

#[test]
fn reservations_cannot_be_promoted_on_another_receiver_owner() {
    let now = Instant::now();
    let first_owner = ReceiverAdmission::new(context(), now);
    let second_owner = ReceiverAdmission::new(context(), now);
    let drops = Arc::new(AtomicUsize::new(0));
    let mut first = admit(&first_owner, now, &drops).unwrap();
    let mut second = admit(&second_owner, now, &drops).unwrap();
    let reservation = first.reserve_pair(Some(context()), now).unwrap();
    assert_eq!(
        second.promote_pair(reservation, Some(context()), now),
        Err(ReceiverError::InvalidContext)
    );
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    drop(first.reserve_pair(Some(context()), now).unwrap());
}

#[test]
fn a_reservation_is_bound_to_its_original_connection_even_within_one_receiver() {
    let now = Instant::now();
    let admission = ReceiverAdmission::new(context(), now);
    let drops = Arc::new(AtomicUsize::new(0));
    let mut first = admit(&admission, now, &drops).unwrap();
    let mut second = admit(&admission, now, &drops).unwrap();
    let reservation = first.reserve_pair(Some(context()), now).unwrap();
    assert_eq!(
        second.promote_pair(reservation, Some(context()), now),
        Err(ReceiverError::InvalidContext)
    );
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    drop(first.reserve_pair(Some(context()), now).unwrap());
}

#[test]
fn closing_the_admission_owner_still_invalidates_a_promoted_connection() {
    let now = Instant::now();
    let admission = ReceiverAdmission::new(context(), now);
    let drops = Arc::new(AtomicUsize::new(0));
    let mut pending = admit(&admission, now, &drops).unwrap();
    let reservation = pending.reserve_pair(Some(context()), now).unwrap();
    pending
        .promote_pair(reservation, Some(context()), now)
        .unwrap();
    admission.close();
    assert_eq!(
        pending.check_context_at(Some(context()), now),
        Err(ReceiverError::Closed)
    );
    assert_eq!(drops.load(Ordering::Relaxed), 1);
}

#[test]
fn phase_deadlines_can_change_only_before_the_previous_budget_expires() {
    let now = Instant::now();
    let admission = ReceiverAdmission::new(context(), now);
    let drops = Arc::new(AtomicUsize::new(0));
    let mut pending = admit(&admission, now, &drops).unwrap();
    let auth_start = now + Duration::from_secs(9);
    pending
        .enter_control_phase(
            Some(context()),
            auth_start,
            auth_start + Duration::from_secs(10),
        )
        .unwrap();
    pending
        .check_context_at(Some(context()), now + Duration::from_secs(18))
        .unwrap();
    assert_eq!(
        pending.enter_control_phase(
            Some(context()),
            now + Duration::from_secs(19),
            now + Duration::from_secs(29)
        ),
        Err(ReceiverError::DeadlineExceeded)
    );
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    let mut pending = admit(&admission, now, &drops).unwrap();
    assert_eq!(
        pending.enter_control_phase(Some(context()), now, now + Duration::from_secs(11)),
        Err(ReceiverError::DeadlineExceeded)
    );
    assert_eq!(drops.load(Ordering::Relaxed), 2);
}

#[test]
fn concurrent_valid_authentication_can_reserve_only_one_paired_slot() {
    let now = Instant::now();
    let admission = ReceiverAdmission::new(context(), now);
    let drops = Arc::new(AtomicUsize::new(0));
    let first = admit(&admission, now, &drops).unwrap();
    let second = admit(&admission, now, &drops).unwrap();
    let barrier = Barrier::new(3);
    let successes = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for mut pending in [first, second] {
            let (barrier, successes) = (&barrier, &successes);
            scope.spawn(move || {
                let reservation = pending.reserve_pair(Some(context()), now);
                match &reservation {
                    Ok(_) => {
                        successes.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(error) => assert_eq!(*error, ReceiverError::Busy),
                }
                barrier.wait();
                barrier.wait();
                drop(reservation);
            });
        }
        barrier.wait();
        assert_eq!(successes.load(Ordering::Relaxed), 1);
        barrier.wait();
    });
}
