use serde::Deserialize;
use serde_json::value::RawValue;

use crate::contract::{ABI_VERSION, MAX_COMMAND_BYTES, MAX_JSON_DEPTH, NativeStatus};

pub(crate) const ZERO_ID: &str = "00000000000000000000000000000000";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    abi: i32,
    kind: String,
    request_id: String,
    generation: String,
    session_id: String,
    transfer_id: String,
    data: Box<RawValue>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyData {}

#[derive(Debug)]
pub(crate) enum Command {
    Start {
        request_id: String,
    },
    Close {
        request_id: String,
        generation: u64,
        session_id: String,
    },
}

pub(crate) fn is_id(value: &str, allow_zero: bool) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        && (allow_zero || value != ZERO_ID)
}

fn check_depth(bytes: &[u8]) -> Result<(), NativeStatus> {
    let (mut depth, mut quoted, mut escaped) = (0usize, false, false);
    for &byte in bytes {
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
        } else {
            match byte {
                b'"' => quoted = true,
                b'{' | b'[' => {
                    depth += 1;
                    if depth > MAX_JSON_DEPTH {
                        return Err(NativeStatus::InvalidArgument);
                    }
                }
                b'}' | b']' => {
                    depth = depth.checked_sub(1).ok_or(NativeStatus::InvalidArgument)?;
                }
                _ => {}
            }
        }
    }
    if quoted || depth != 0 {
        return Err(NativeStatus::InvalidArgument);
    }
    Ok(())
}

pub(crate) fn parse(bytes: &[u8]) -> Result<Command, NativeStatus> {
    if bytes.is_empty() || bytes.len() > MAX_COMMAND_BYTES {
        return Err(NativeStatus::InvalidArgument);
    }
    check_depth(bytes)?;
    let value: Envelope =
        serde_json::from_slice(bytes).map_err(|_| NativeStatus::InvalidArgument)?;
    if value.abi != ABI_VERSION {
        return Err(NativeStatus::AbiMismatch);
    }
    if !is_id(&value.request_id, false)
        || !is_id(&value.session_id, true)
        || !is_id(&value.transfer_id, true)
        || value.generation.is_empty()
        || (value.generation.len() > 1 && value.generation.starts_with('0'))
        || !value.generation.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(NativeStatus::InvalidArgument);
    }
    let generation: u64 = value
        .generation
        .parse()
        .map_err(|_| NativeStatus::InvalidArgument)?;
    if generation > i64::MAX as u64 {
        return Err(NativeStatus::InvalidArgument);
    }
    match value.kind.as_str() {
        "startReceiveSession" | "closeSession" => {}
        "createInvitation"
        | "connectAndPair"
        | "offerTransfer"
        | "approveOffer"
        | "declineOffer"
        | "confirmDestinationCommit"
        | "cancelTransfer" => return Err(NativeStatus::InvalidState),
        _ => return Err(NativeStatus::InvalidArgument),
    }
    let _: EmptyData =
        serde_json::from_str(value.data.get()).map_err(|_| NativeStatus::InvalidArgument)?;
    if value.transfer_id != ZERO_ID {
        return Err(NativeStatus::InvalidState);
    }
    if value.kind == "startReceiveSession" {
        if generation != 0 || value.session_id != ZERO_ID {
            return Err(NativeStatus::InvalidState);
        }
        Ok(Command::Start {
            request_id: value.request_id,
        })
    } else {
        if generation == 0 || value.session_id == ZERO_ID {
            return Err(NativeStatus::InvalidState);
        }
        Ok(Command::Close {
            request_id: value.request_id,
            generation,
            session_id: value.session_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn start() -> String {
        format!(
            r#"{{"abi":1,"kind":"startReceiveSession","request_id":"{}","generation":"0","session_id":"{ZERO_ID}","transfer_id":"{ZERO_ID}","data":{{}}}}"#,
            "11".repeat(16)
        )
    }
    #[test]
    fn rejects_duplicate_unknown_and_oversized_commands() {
        assert!(parse(start().as_bytes()).is_ok());
        let duplicate = start().replacen("\"abi\":1", "\"abi\":1,\"abi\":1", 1);
        assert!(matches!(
            parse(duplicate.as_bytes()),
            Err(NativeStatus::InvalidArgument)
        ));
        let unknown = start().replacen("\"abi\":1", "\"extra\":true,\"abi\":1", 1);
        assert!(matches!(
            parse(unknown.as_bytes()),
            Err(NativeStatus::InvalidArgument)
        ));
        assert!(matches!(
            parse(&vec![b' '; MAX_COMMAND_BYTES + 1]),
            Err(NativeStatus::InvalidArgument)
        ));
        assert!(matches!(
            parse(start().replace("\"0\"", "\"00\"").as_bytes()),
            Err(NativeStatus::InvalidArgument)
        ));
        assert!(matches!(
            parse(
                start()
                    .replace("\"data\":{}", "\"data\":{\"path\":\"/tmp/no\"}")
                    .as_bytes()
            ),
            Err(NativeStatus::InvalidArgument)
        ));
    }
    #[test]
    fn rejects_depth_and_unimplemented_pairing_without_success() {
        assert!(matches!(
            parse(
                start()
                    .replace("\"data\":{}", "\"data\":[[[[[[[]]]]]]]")
                    .as_bytes()
            ),
            Err(NativeStatus::InvalidArgument)
        ));
        assert!(matches!(
            parse(
                start()
                    .replace("startReceiveSession", "createInvitation")
                    .as_bytes()
            ),
            Err(NativeStatus::InvalidState)
        ));
    }
}
