use super::*;
use crate::command::{self, Command, ZERO_ID};
use serde_json::{Value, json};

fn input() -> Value {
    json!({"route_generation":"1", "network_handle":"18446744073709551615", "interface_index":7,
        "locals":[{"address":"c0a82a01", "scope_id":"0", "prefix_length":24}],
        "client_addresses":[], "client_interfaces":[]})
}
fn parse(value: Value) -> ReceiverRouteInput {
    serde_json::from_value(value).unwrap()
}

#[test]
fn preserves_full_network_bits_and_separate_generations() {
    use crate::transport::{ReceiverContext, ReceiverRoute, ReceiverRoutes};
    let input = parse(input());
    let context = ReceiverContext::new(3, 1, [0x11; 16]).unwrap();
    let expected = ReceiverRoutes::wifi(
        context,
        u64::MAX,
        7,
        &[ReceiverRoute::new(&[192, 168, 42, 1], 0, 24).unwrap()],
    )
    .unwrap();
    assert_eq!(input.routes(3, &"11".repeat(16)).unwrap(), expected);
    assert!(!format!("{input:?}").contains("c0a82a01"));
}

#[test]
fn hotspot_has_no_fabricated_network_and_keeps_exclusions() {
    let mut value = input();
    value["network_handle"] = json!("0");
    value["client_addresses"] = json!(["0a000002", "fe800000000000000000000000000002"]);
    value["client_interfaces"] = json!([8, 9]);
    assert!(parse(value.clone()).routes(1, &"11".repeat(16)).is_ok());
    value["client_addresses"] = json!(["c0a82a01"]);
    assert_eq!(
        parse(value).routes(1, &"11".repeat(16)),
        Err(NativeStatus::InvalidArgument)
    );
}

#[test]
fn rejects_noncanonical_numbers_sizes_and_cross_mode_exclusions() {
    for (field, bad) in [
        ("network_handle", json!("18446744073709551616")),
        ("network_handle", json!("-1")),
        ("route_generation", json!("01")),
        ("route_generation", json!("0")),
        ("route_generation", json!("9223372036854775808")),
        ("interface_index", json!(0)),
        ("locals", json!([])),
        ("client_interfaces", json!([8])),
    ] {
        let mut value = input();
        value[field] = bad;
        assert!(parse(value).validate().is_err(), "{field}");
    }
    let mut value = input();
    value["locals"] = json!(vec![value["locals"][0].clone(); 17]);
    assert!(parse(value).validate().is_err());
    let mut value = input();
    value["network_handle"] = json!("0");
    value["client_addresses"] = json!(vec!["0a000002"; 129]);
    assert!(parse(value).validate().is_err());
}

#[test]
fn rejects_invalid_addresses_scopes_and_prefixes_before_listener() {
    for (field, bad) in [
        ("address", json!("C0A82A01")),
        ("address", json!("127.0.0.1")),
        ("scope_id", json!("01")),
        ("scope_id", json!("4294967296")),
        ("prefix_length", json!(0)),
        ("prefix_length", json!(33)),
        ("address", json!("7f000001")),
        ("scope_id", json!("7")),
        ("address", json!("c0a82a00")),
    ] {
        let mut value = input();
        value["locals"][0][field] = bad;
        assert!(parse(value).routes(1, &"11".repeat(16)).is_err(), "{field}");
    }
    let mut value = input();
    value["locals"][0] =
        json!({"address":"fe800000000000000000000000000001", "scope_id":"7", "prefix_length":64});
    assert!(parse(value.clone()).routes(1, &"11".repeat(16)).is_ok());
    value["locals"][0]["scope_id"] = json!("8");
    assert!(parse(value).routes(1, &"11".repeat(16)).is_err());
}

#[test]
fn command_rejects_unknown_duplicate_fields_and_stale_shape() {
    let mut value = json!({"abi":1,"kind":"attachReceiver","request_id":"11".repeat(16),
        "generation":"1","session_id":"22".repeat(16),"transfer_id":ZERO_ID,"data":input()});
    assert!(matches!(
        command::parse(value.to_string().as_bytes()),
        Ok(Command::Receiver { update: false, .. })
    ));
    value["kind"] = json!("updateReceiverRoutes");
    assert!(matches!(
        command::parse(value.to_string().as_bytes()),
        Ok(Command::Receiver { update: true, .. })
    ));
    let duplicate = value.to_string().replace(
        "\"route_generation\":\"1\"",
        "\"route_generation\":\"1\",\"route_generation\":\"2\"",
    );
    assert!(matches!(
        command::parse(duplicate.as_bytes()),
        Err(NativeStatus::InvalidArgument)
    ));
    value["data"]["locals"][0]["url"] = json!("http://example.invalid");
    assert!(matches!(
        command::parse(value.to_string().as_bytes()),
        Err(NativeStatus::InvalidArgument)
    ));
    value["data"] = input();
    value["generation"] = json!("0");
    assert!(matches!(
        command::parse(value.to_string().as_bytes()),
        Err(NativeStatus::InvalidState)
    ));
}
