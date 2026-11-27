use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6};

use nuvyro_transfer_core::protocol::TRANSFER_PORT;

use super::ConnectError;

/// Numeric address only. Android must also validate subnet, Network, epoch, and QR selection.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct NumericEndpoint(SocketAddr);

impl NumericEndpoint {
    pub(crate) fn new(address: &[u8], scope_id: u32, port: u16) -> Result<Self, ConnectError> {
        if port != TRANSFER_PORT {
            return Err(ConnectError::InvalidEndpoint);
        }
        let socket = match address {
            [a, b, c, d] => {
                let ip = Ipv4Addr::new(*a, *b, *c, *d);
                if scope_id != 0
                    || *a == 0
                    || *a >= 224
                    || ip.is_loopback()
                    || ip.is_unspecified()
                    || ip.is_multicast()
                {
                    return Err(ConnectError::InvalidEndpoint);
                }
                SocketAddr::new(IpAddr::V4(ip), port)
            }
            bytes if bytes.len() == 16 => {
                let octets: [u8; 16] = bytes
                    .try_into()
                    .map_err(|_| ConnectError::InvalidEndpoint)?;
                let ip = Ipv6Addr::from(octets);
                let compatible = octets[..12] == [0; 12];
                let mapped = octets[..10] == [0; 10] && octets[10..12] == [0xff; 2];
                let link_local = ip.is_unicast_link_local();
                if ip.is_unspecified()
                    || ip.is_loopback()
                    || ip.is_multicast()
                    || compatible
                    || mapped
                    || (link_local && (scope_id == 0 || scope_id > i32::MAX as u32))
                    || (!link_local && scope_id != 0)
                {
                    return Err(ConnectError::InvalidEndpoint);
                }
                SocketAddr::V6(SocketAddrV6::new(ip, port, 0, scope_id))
            }
            _ => return Err(ConnectError::InvalidEndpoint),
        };
        Ok(Self(socket))
    }

    pub(super) const fn address(self) -> SocketAddr {
        self.0
    }
}

impl fmt::Debug for NumericEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NumericEndpoint")
            .field("ipv6", &self.0.is_ipv6())
            .finish_non_exhaustive()
    }
}
