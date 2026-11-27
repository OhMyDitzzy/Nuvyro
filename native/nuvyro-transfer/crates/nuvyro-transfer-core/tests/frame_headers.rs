use nuvyro_transfer_core::protocol::{FrameHeader, HeaderError};

#[test]
fn fixed_header_vectors_enforce_type_and_allocation_bounds() {
    let fixtures = include_str!("fixtures/frame-headers.txt");
    for line in fixtures.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let columns: Vec<_> = line.split_whitespace().collect();
        assert_eq!(columns.len(), 3, "fixture: {line}");
        let name = columns[0];
        let encoded = columns[1];
        assert_eq!(encoded.len(), 12, "fixture: {name}");
        let mut bytes = [0; 6];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&encoded[index * 2..index * 2 + 2], 16)
                .expect("fixture hex byte");
        }
        let result = FrameHeader::decode(&bytes);
        match columns[2] {
            "length" => assert_eq!(result, Err(HeaderError::InvalidLength), "{name}"),
            "unknown" => assert!(
                matches!(result, Err(HeaderError::UnknownMessageType(_))),
                "{name}: {result:?}"
            ),
            expected => {
                let fields: Vec<_> = expected.split(':').collect();
                assert_eq!(fields.len(), 3, "fixture: {name}");
                assert_eq!(fields[0], "ok", "fixture: {name}");
                let kind = u16::from_str_radix(fields[1], 16).expect("fixture kind");
                let body_len = fields[2].parse::<u32>().expect("fixture body length");
                let header = result.unwrap_or_else(|error| panic!("{name}: {error:?}"));
                assert_eq!(header.kind() as u16, kind, "{name}");
                assert_eq!(header.body_len(), body_len, "{name}");
            }
        }
    }
}
