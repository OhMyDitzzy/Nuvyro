use serde::Serialize;

use crate::command::ZERO_ID;
use crate::contract::{ABI_VERSION, MAX_EVENT_BYTES, NativeStatus};

#[derive(Serialize)]
struct Event<'a> {
    abi: i32,
    kind: &'a str,
    event_id: String,
    request_id: &'a str,
    generation: String,
    session_id: &'a str,
    transfer_id: &'a str,
    data: serde_json::Value,
}

pub(crate) fn lifecycle(
    kind: &str,
    event_id: u64,
    request: &str,
    generation: u64,
    session: &str,
) -> Result<Vec<u8>, NativeStatus> {
    let event = Event {
        abi: ABI_VERSION,
        kind,
        event_id: format!("{event_id:032x}"),
        request_id: request,
        generation: generation.to_string(),
        session_id: session,
        transfer_id: ZERO_ID,
        data: serde_json::json!({"transport_ready": false}),
    };
    let encoded = serde_json::to_vec(&event).map_err(|_| NativeStatus::InternalFailure)?;
    if encoded.len() > MAX_EVENT_BYTES {
        return Err(NativeStatus::ResourceLimit);
    }
    Ok(encoded)
}
