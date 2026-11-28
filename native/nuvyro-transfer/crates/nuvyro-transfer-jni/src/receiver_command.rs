use serde::Deserialize;

use crate::contract::NativeStatus;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReceiverRouteInput {
    route_generation: String,
    network_handle: String,
    interface_index: u32,
    locals: Vec<LocalInput>,
    client_addresses: Vec<String>,
    client_interfaces: Vec<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalInput {
    address: String,
    scope_id: String,
    prefix_length: u8,
}

pub(crate) fn decimal(value: &str) -> Result<u64, NativeStatus> {
    if value.is_empty()
        || value.len() > 20
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(NativeStatus::InvalidArgument);
    }
    value.parse().map_err(|_| NativeStatus::InvalidArgument)
}

fn address(value: &str) -> Result<Vec<u8>, NativeStatus> {
    if !matches!(value.len(), 8 | 32)
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(NativeStatus::InvalidArgument);
    }
    (0..value.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&value[i..i + 2], 16).map_err(|_| NativeStatus::InvalidArgument)
        })
        .collect()
}

impl ReceiverRouteInput {
    pub(crate) fn validate(&self) -> Result<(), NativeStatus> {
        let generation = decimal(&self.route_generation)?;
        let network = decimal(&self.network_handle)?;
        if generation == 0
            || generation > i64::MAX as u64
            || self.interface_index == 0
            || self.interface_index > i32::MAX as u32
            || self.locals.is_empty()
            || self.locals.len() > 16
            || self.client_addresses.len() > 128
            || self.client_interfaces.len() > 32
            || self
                .client_interfaces
                .iter()
                .any(|i| *i == 0 || *i > i32::MAX as u32 || *i == self.interface_index)
            || (network != 0
                && (!self.client_addresses.is_empty() || !self.client_interfaces.is_empty()))
        {
            return Err(NativeStatus::InvalidArgument);
        }
        for local in &self.locals {
            let bytes = address(&local.address)?;
            let scope = decimal(&local.scope_id)?;
            if scope > i32::MAX as u64
                || local.prefix_length == 0
                || local.prefix_length as usize > bytes.len() * 8
            {
                return Err(NativeStatus::InvalidArgument);
            }
        }
        for client in &self.client_addresses {
            address(client)?;
        }
        Ok(())
    }

    #[cfg(any(target_os = "android", test))]
    pub(crate) fn routes(
        &self,
        generation: u64,
        session: &str,
    ) -> Result<crate::transport::ReceiverRoutes, NativeStatus> {
        use crate::transport::{ReceiverContext, ReceiverRoute, ReceiverRoutes};
        self.validate()?;
        let session: [u8; 16] = address(session)?
            .try_into()
            .map_err(|_| NativeStatus::InvalidArgument)?;
        let context = ReceiverContext::new(generation, decimal(&self.route_generation)?, session)
            .map_err(|_| NativeStatus::InvalidArgument)?;
        let locals = self
            .locals
            .iter()
            .map(|local| {
                ReceiverRoute::new(
                    &address(&local.address)?,
                    decimal(&local.scope_id)? as u32,
                    local.prefix_length,
                )
                .map_err(|_| NativeStatus::InvalidArgument)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let network = decimal(&self.network_handle)?;
        let routes = if network != 0 {
            ReceiverRoutes::wifi(context, network, self.interface_index, &locals)
        } else {
            let clients = self
                .client_addresses
                .iter()
                .map(|value| {
                    let bytes = address(value)?;
                    Ok(match bytes.len() {
                        4 => std::net::IpAddr::V4(std::net::Ipv4Addr::from(
                            <[u8; 4]>::try_from(bytes).unwrap(),
                        )),
                        _ => std::net::IpAddr::V6(std::net::Ipv6Addr::from(
                            <[u8; 16]>::try_from(bytes).unwrap(),
                        )),
                    })
                })
                .collect::<Result<Vec<_>, NativeStatus>>()?;
            ReceiverRoutes::hotspot(
                context,
                self.interface_index,
                &locals,
                &clients,
                &self.client_interfaces,
            )
        };
        routes.map_err(|_| NativeStatus::InvalidArgument)
    }
}

impl std::fmt::Debug for ReceiverRouteInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReceiverRouteInput(..)")
    }
}

#[cfg(test)]
mod tests;
