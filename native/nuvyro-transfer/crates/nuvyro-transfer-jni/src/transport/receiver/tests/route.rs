use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6};

use super::context;
use crate::transport::receiver::{ReceiverRoute, ReceiverRoutes};

fn v4(last: u8) -> ReceiverRoute {
    ReceiverRoute::new(&[192, 168, 42, last], 0, 24).unwrap()
}
fn local() -> SocketAddr {
    "192.168.42.1:47739".parse().unwrap()
}
fn peer(last: u8) -> SocketAddr {
    SocketAddr::from(([192, 168, 42, last], 40123))
}

#[test]
fn only_unicast_numeric_local_routes_with_real_prefixes_are_constructible() {
    for ip in [
        [0, 1, 2, 3],
        [127, 0, 0, 1],
        [224, 0, 0, 1],
        [255, 255, 255, 255],
        [192, 168, 42, 0],
        [192, 168, 42, 255],
    ] {
        assert!(ReceiverRoute::new(&ip, 0, 24).is_err());
    }
    assert!(ReceiverRoute::new(&[192, 168, 42, 1], 0, 0).is_err());
    assert!(ReceiverRoute::new(&[192, 168, 42, 1], 0, 33).is_err());
    assert!(ReceiverRoute::new(&[192, 168, 42, 1], 7, 24).is_err());
    assert!(ReceiverRoute::new(&[1, 2, 3], 0, 24).is_err());
    assert_eq!(v4(1).local(), local());
    assert_eq!(format!("{:?}", v4(1)), "ReceiverRoute { .. }");
}

#[test]
fn wifi_requires_a_network_but_preserves_all_unsigned_handle_bits() {
    assert!(ReceiverRoutes::wifi(context(), 0, 7, &[v4(1)]).is_err());
    let routes = ReceiverRoutes::wifi(context(), u64::MAX, 7, &[v4(1)]).unwrap();
    assert_eq!(routes.network(), Some(u64::MAX));
    assert_eq!(routes.context(), context());
    assert_eq!(format!("{routes:?}"), "ReceiverRoutes { .. }");
}

#[test]
fn routes_reject_invalid_interfaces_oversize_snapshots_and_contradicting_prefixes() {
    for index in [0, u32::MAX] {
        assert!(ReceiverRoutes::wifi(context(), 1, index, &[v4(1)]).is_err());
    }
    assert!(ReceiverRoutes::wifi(context(), 1, 7, &[]).is_err());
    assert!(ReceiverRoutes::wifi(context(), 1, 7, &[v4(1); 17]).is_err());
    let alias = ReceiverRoute::new(&[192, 168, 42, 1], 0, 16).unwrap();
    assert!(ReceiverRoutes::wifi(context(), 1, 7, &[v4(1), alias]).is_err());
    assert!(
        ReceiverRoutes::hotspot(
            context(),
            7,
            &[v4(1)],
            &[IpAddr::V4(Ipv4Addr::LOCALHOST); 129],
            &[]
        )
        .is_err()
    );
    assert!(ReceiverRoutes::hotspot(context(), 7, &[v4(1)], &[], &[1; 33]).is_err());
    assert!(ReceiverRoutes::hotspot(context(), 7, &[v4(1)], &[], &[0]).is_err());
}

#[test]
fn hotspot_needs_no_client_network_and_rejects_every_known_client_interface_or_address() {
    let routes = ReceiverRoutes::hotspot(context(), 7, &[v4(1)], &[], &[]).unwrap();
    assert_eq!(routes.network(), None);
    assert!(ReceiverRoutes::hotspot(context(), 7, &[v4(1)], &[], &[7]).is_err());
    assert!(ReceiverRoutes::hotspot(context(), 7, &[v4(1)], &[local().ip()], &[]).is_err());
    let excluded = ReceiverRoutes::hotspot(context(), 7, &[v4(1)], &[peer(2).ip()], &[8]).unwrap();
    assert!(!excluded.allows(local(), peer(2)));
    assert!(excluded.allows(local(), peer(3)));
}

#[test]
fn accepted_local_address_port_peer_subnet_and_self_addresses_must_all_match() {
    let routes = ReceiverRoutes::wifi(context(), 1, 7, &[v4(1), v4(2)]).unwrap();
    assert!(routes.allows(local(), peer(3)));
    for remote in [
        peer(0),
        peer(255),
        peer(1),
        peer(2),
        "192.168.43.3:7".parse().unwrap(),
        "127.0.0.1:7".parse().unwrap(),
        "224.0.0.1:7".parse().unwrap(),
        "0.0.0.0:7".parse().unwrap(),
        "192.168.42.3:0".parse().unwrap(),
    ] {
        assert!(!routes.allows(local(), remote));
    }
    for address in ["0.0.0.0:47739", "192.168.42.1:47738", "192.168.42.4:47739"] {
        assert!(!routes.allows(address.parse().unwrap(), peer(3)));
    }
}

#[test]
fn most_specific_prefix_prevents_broadcast_reclassification_and_partial_bits_work() {
    let a = ReceiverRoute::new(&[10, 0, 0, 1], 0, 8).unwrap();
    let b = ReceiverRoute::new(&[10, 1, 2, 1], 0, 24).unwrap();
    let routes = ReceiverRoutes::wifi(context(), 1, 7, &[a, b]).unwrap();
    assert!(!routes.allows(a.local(), SocketAddr::from(([10, 1, 2, 255], 9))));
    assert!(!routes.allows(a.local(), SocketAddr::from(([10, 1, 2, 0], 9))));
    assert!(routes.allows(a.local(), SocketAddr::from(([10, 1, 2, 3], 9))));
    let subnet = ReceiverRoute::new(&[10, 1, 128, 1], 0, 17).unwrap();
    let routes = ReceiverRoutes::wifi(context(), 1, 7, &[subnet]).unwrap();
    assert!(routes.allows(subnet.local(), SocketAddr::from(([10, 1, 255, 3], 9))));
    assert!(!routes.allows(subnet.local(), SocketAddr::from(([10, 1, 127, 3], 9))));
}

#[test]
fn point_to_point_ipv4_does_not_treat_its_only_peer_as_broadcast() {
    let bound = ReceiverRoute::new(&[192, 168, 42, 2], 0, 31).unwrap();
    let routes = ReceiverRoutes::wifi(context(), 1, 7, &[bound]).unwrap();
    assert!(routes.allows(bound.local(), peer(3)));
    assert!(!routes.allows(bound.local(), peer(2)));
}

#[test]
fn ipv6_link_scope_must_identify_the_selected_interface_and_families_never_alias() {
    let ip: Ipv6Addr = "fe80::1".parse().unwrap();
    assert!(ReceiverRoute::new(&ip.octets(), 0, 64).is_err());
    let route = ReceiverRoute::new(&ip.octets(), 7, 64).unwrap();
    assert!(ReceiverRoutes::wifi(context(), 1, 8, &[route]).is_err());
    let routes = ReceiverRoutes::wifi(context(), 1, 7, &[route]).unwrap();
    for index in [0, 8] {
        let remote = SocketAddr::V6(SocketAddrV6::new("fe80::2".parse().unwrap(), 9, 0, index));
        assert!(!routes.allows(route.local(), remote));
    }
    assert!(routes.allows(
        route.local(),
        SocketAddr::V6(SocketAddrV6::new("fe80::2".parse().unwrap(), 9, 0, 7))
    ));
    for bytes in [
        Ipv6Addr::UNSPECIFIED.octets(),
        Ipv6Addr::LOCALHOST.octets(),
        "::ffff:192.168.42.3".parse::<Ipv6Addr>().unwrap().octets(),
        "::192.168.42.3".parse::<Ipv6Addr>().unwrap().octets(),
    ] {
        assert!(ReceiverRoute::new(&bytes, 0, 64).is_err());
    }
}

#[test]
fn all_local_bindings_are_bounded_and_canonical_without_dropping_self_filters() {
    let ipv6 =
        ReceiverRoute::new(&"2001:db8::1".parse::<Ipv6Addr>().unwrap().octets(), 0, 64).unwrap();
    let a = ReceiverRoutes::wifi(context(), 1, 7, &[v4(2), ipv6, v4(1)]).unwrap();
    let b = ReceiverRoutes::wifi(context(), 1, 7, &[v4(1), v4(2), ipv6]).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.bindings().len(), 3);
    assert_eq!(a.bindings()[0].local(), local());
    assert!(!a.allows(local(), peer(2)));
    assert_ne!(
        a,
        ReceiverRoutes::wifi(context(), 2, 7, &[v4(1), v4(2), ipv6]).unwrap()
    );
    assert_ne!(
        a,
        ReceiverRoutes::hotspot(context(), 7, &[v4(1), v4(2), ipv6], &[], &[]).unwrap()
    );
}
