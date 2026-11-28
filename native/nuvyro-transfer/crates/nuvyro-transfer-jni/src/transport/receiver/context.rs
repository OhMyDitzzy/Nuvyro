use std::num::NonZeroU64;

use nuvyro_transfer_core::protocol::Id;

use super::ReceiverError;

/// Listener ownership, not peer authentication or Android network provenance.
/// AP hosts need not have a client Network handle. The listener must validate its route.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct ReceiverContext {
    native_generation: NonZeroU64,
    route_generation: NonZeroU64,
    session: Id,
}

impl ReceiverContext {
    pub(crate) fn new(
        native_generation: u64,
        route_generation: u64,
        session: [u8; 16],
    ) -> Result<Self, ReceiverError> {
        if native_generation > i64::MAX as u64 || route_generation > i64::MAX as u64 {
            return Err(ReceiverError::InvalidContext);
        }
        Ok(Self {
            native_generation: NonZeroU64::new(native_generation)
                .ok_or(ReceiverError::InvalidContext)?,
            route_generation: NonZeroU64::new(route_generation)
                .ok_or(ReceiverError::InvalidContext)?,
            session: Id::from_bytes(session).map_err(|_| ReceiverError::InvalidContext)?,
        })
    }

    pub(crate) const fn session(self) -> Id {
        self.session
    }

    pub(crate) const fn native_generation(self) -> u64 {
        self.native_generation.get()
    }

    pub(crate) const fn route_generation(self) -> u64 {
        self.route_generation.get()
    }
}

impl std::fmt::Debug for ReceiverContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReceiverContext").finish_non_exhaustive()
    }
}
