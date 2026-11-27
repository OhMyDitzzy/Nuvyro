use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU64;
use std::sync::Arc;

use nuvyro_transfer_core::protocol::TRANSFER_PORT;

use super::{ListenerError, ReceiverContext};
use crate::transport::NumericEndpoint;

const MAX_LOCALS: usize = 16;
const MAX_CLIENT_ADDRESSES: usize = 128;
const MAX_CLIENT_INTERFACES: usize = 32;

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct ReceiverRoute {
    local: SocketAddr,
    prefix: u8,
}

impl ReceiverRoute {
    pub(crate) fn new(address: &[u8], scope: u32, prefix: u8) -> Result<Self, ListenerError> {
        let local = NumericEndpoint::new(address, scope, TRANSFER_PORT)
            .map_err(|_| ListenerError::InvalidRoute)?
            .address();
        let bits = if local.is_ipv4() { 32 } else { 128 };
        if prefix == 0 || prefix > bits || ipv4_boundary(local.ip(), prefix) {
            return Err(ListenerError::InvalidRoute);
        }
        Ok(Self { local, prefix })
    }

    pub(in crate::transport) const fn local(self) -> SocketAddr {
        self.local
    }
}

#[derive(Eq, PartialEq)]
struct Routes {
    context: ReceiverContext,
    interface: u32,
    network: Option<NonZeroU64>,
    locals: Vec<ReceiverRoute>,
    client_addresses: Vec<IpAddr>,
    client_interfaces: Vec<u32>,
}

/// A trusted platform owner's immutable routing snapshot, not peer authentication.
/// The owner must refresh it on interface/Network/hotspot changes and fail on incomplete reads.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct ReceiverRoutes(Arc<Routes>);

impl ReceiverRoutes {
    pub(crate) fn wifi(
        context: ReceiverContext,
        network: u64,
        interface: u32,
        locals: &[ReceiverRoute],
    ) -> Result<Self, ListenerError> {
        let network = NonZeroU64::new(network).ok_or(ListenerError::InvalidRoute)?;
        Self::create(context, interface, Some(network), locals, &[], &[])
    }

    /// The caller owns an active local-only hotspot reservation and supplies complete current
    /// ConnectivityManager client addresses/interfaces, including VPN and cellular uplinks.
    pub(crate) fn hotspot(
        context: ReceiverContext,
        interface: u32,
        locals: &[ReceiverRoute],
        client_addresses: &[IpAddr],
        client_interfaces: &[u32],
    ) -> Result<Self, ListenerError> {
        Self::create(
            context,
            interface,
            None,
            locals,
            client_addresses,
            client_interfaces,
        )
    }

    fn create(
        context: ReceiverContext,
        interface: u32,
        network: Option<NonZeroU64>,
        locals: &[ReceiverRoute],
        client_addresses: &[IpAddr],
        client_interfaces: &[u32],
    ) -> Result<Self, ListenerError> {
        if interface == 0
            || interface > i32::MAX as u32
            || locals.is_empty()
            || locals.len() > MAX_LOCALS
            || client_addresses.len() > MAX_CLIENT_ADDRESSES
            || client_interfaces.len() > MAX_CLIENT_INTERFACES
            || client_interfaces.contains(&interface)
            || client_interfaces
                .iter()
                .any(|index| *index == 0 || *index > i32::MAX as u32)
            || locals.iter().any(|route| {
                client_addresses.contains(&route.local.ip())
                    || (link_local(route.local.ip()) && scope(route.local) != interface)
            })
        {
            return Err(ListenerError::InvalidRoute);
        }
        let mut all = bounded_copy(locals)?;
        all.sort_by_key(|route| (route.local, route.prefix));
        all.dedup();
        // Never choose a weaker prefix for the same local binding.
        if all.windows(2).any(|pair| pair[0].local == pair[1].local) {
            return Err(ListenerError::InvalidRoute);
        }
        let mut client_addresses = bounded_copy(client_addresses)?;
        client_addresses.sort();
        client_addresses.dedup();
        let mut client_interfaces = bounded_copy(client_interfaces)?;
        client_interfaces.sort();
        client_interfaces.dedup();
        Ok(Self(Arc::new(Routes {
            context,
            interface,
            network,
            locals: all,
            client_addresses,
            client_interfaces,
        })))
    }

    pub(crate) fn context(&self) -> ReceiverContext {
        self.0.context
    }
    pub(in crate::transport) fn network(&self) -> Option<u64> {
        self.0.network.map(NonZeroU64::get)
    }
    pub(in crate::transport) fn bindings(&self) -> &[ReceiverRoute] {
        &self.0.locals
    }

    pub(in crate::transport) fn allows(&self, local: SocketAddr, peer: SocketAddr) -> bool {
        if !self.0.locals.iter().any(|route| route.local == local)
            || peer.port() == 0
            || !valid_peer(peer, self.0.interface)
            || self
                .0
                .locals
                .iter()
                .any(|route| route.local.ip() == peer.ip())
            || self.0.client_addresses.contains(&peer.ip())
        {
            return false;
        }
        let Some(prefix) = self
            .0
            .locals
            .iter()
            .filter(|route| matches_prefix(peer.ip(), route.local.ip(), route.prefix))
            .map(|route| route.prefix)
            .max()
        else {
            return false;
        };
        !ipv4_boundary(peer.ip(), prefix)
    }
}

fn bounded_copy<T: Copy>(values: &[T]) -> Result<Vec<T>, ListenerError> {
    let mut out = Vec::new();
    out.try_reserve_exact(values.len())
        .map_err(|_| ListenerError::ResourceLimit)?;
    out.extend_from_slice(values);
    Ok(out)
}

fn valid_peer(peer: SocketAddr, interface: u32) -> bool {
    let result = match peer.ip() {
        IpAddr::V4(ip) => NumericEndpoint::new(&ip.octets(), 0, TRANSFER_PORT),
        IpAddr::V6(ip) => NumericEndpoint::new(&ip.octets(), scope(peer), TRANSFER_PORT),
    };
    result.is_ok() && (!link_local(peer.ip()) || scope(peer) == interface)
}

fn link_local(ip: IpAddr) -> bool {
    matches!(ip, IpAddr::V6(ip) if ip.is_unicast_link_local())
}

fn scope(address: SocketAddr) -> u32 {
    match address {
        SocketAddr::V6(v6) => v6.scope_id(),
        _ => 0,
    }
}

fn matches_prefix(peer: IpAddr, local: IpAddr, bits: u8) -> bool {
    match (peer, local) {
        (IpAddr::V4(a), IpAddr::V4(b)) => (u32::from(a) ^ u32::from(b)) >> (32 - bits) == 0,
        (IpAddr::V6(a), IpAddr::V6(b)) => (u128::from(a) ^ u128::from(b)) >> (128 - bits) == 0,
        _ => false,
    }
}

fn ipv4_boundary(ip: IpAddr, bits: u8) -> bool {
    let IpAddr::V4(ip) = ip else {
        return false;
    };
    if bits >= 31 {
        return false;
    }
    let mask = u32::MAX >> bits;
    let host = u32::from(ip) & mask;
    host == 0 || host == mask
}

impl std::fmt::Debug for ReceiverRoute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReceiverRoute").finish_non_exhaustive()
    }
}
impl std::fmt::Debug for ReceiverRoutes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReceiverRoutes").finish_non_exhaustive()
    }
}
