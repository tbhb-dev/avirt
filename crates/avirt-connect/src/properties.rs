//! Property tests for authority parsing and the destination checks.
//!
//! Each test states a rule that must hold for every input proptest generates, rather than for a hand-picked example. Hostnames, IP literals and ports come from small strategies, and a failing case shrinks to the shortest input that still breaks the rule.
//!
//! Totality tests feed arbitrary text to the parsers and require only that nothing panics, since the authority comes from an untrusted agent. Round-trip tests, the second group, render a known host and port, parse the result, and expect the same pair back. A third test compares the verdict for an IPv4 address with the verdict for its IPv6-mapped form, which closes a bypass where a private address hides inside a mapped literal.

use core::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use proptest::collection::vec;
use proptest::prelude::{Strategy, any, prop_assert, prop_assert_eq, proptest};

use super::{Target, absolute_uri_authority, is_private};

/// A lowercase DNS-style hostname of one to four labels.
fn hostname() -> impl Strategy<Value = String> {
    vec("[a-z][a-z0-9_-]{0,20}", 1..=4).prop_map(|labels| labels.join("."))
}

proptest! {
    /// Parsing never panics, whatever the bytes.
    ///
    /// Authority text comes from an untrusted agent, so a panic here would let the agent crash the proxy.
    #[test]
    fn parse_total(authority in any::<String>()) {
        let _parsed = Target::parse(&authority);
    }

    /// A parsed target is lowercase, nonempty, in port range, and parses again from its own rendering.
    #[test]
    fn parsed_target_is_canonical(authority in "[ -~]{0,80}") {
        if let Some(target) = Target::parse(&authority) {
            prop_assert!(!target.host.is_empty());
            prop_assert!(target.port != 0);
            prop_assert_eq!(target.host.clone(), target.host.to_ascii_lowercase());
            let rendered = if target.host.contains(':') {
                format!("[{}]:{}", target.host, target.port)
            } else {
                format!("{}:{}", target.host, target.port)
            };
            prop_assert_eq!(Target::parse(&rendered), Some(target));
        }
    }

    /// Every dotted-quad IPv4 literal with a valid port parses to itself.
    #[test]
    fn ipv4_literal_round_trips(octets in any::<[u8; 4]>(), port in 1u16..) {
        let address = Ipv4Addr::from(octets);
        let target = Target::parse(&format!("{address}:{port}"));
        prop_assert_eq!(target, Some(Target { host: address.to_string(), port }));
    }

    /// Every bracketed IPv6 literal with a valid port parses to its canonical form.
    #[test]
    fn ipv6_literal_round_trips(segments in any::<[u16; 8]>(), port in 1u16..) {
        let address = Ipv6Addr::from(segments);
        let target = Target::parse(&format!("[{address}]:{port}"));
        prop_assert_eq!(target, Some(Target { host: address.to_string(), port }));
    }

    /// Hostname plus valid port: the parse gives the same pair back.
    #[test]
    fn hostname_round_trips(host in hostname(), port in 1u16..) {
        // A name whose last label starts with a digit would read as numeric.
        let target = Target::parse(&format!("{host}:{port}"));
        prop_assert_eq!(target, Some(Target { host, port }));
    }

    /// Port zero is invalid, whatever the host.
    #[test]
    fn port_zero_is_refused(host in hostname()) {
        prop_assert_eq!(Target::parse(&format!("{host}:0")), None);
    }

    /// An IPv4-mapped IPv6 address gets the same verdict as the IPv4 address inside it.
    ///
    /// Otherwise the mapped spelling would bypass the private-address check.
    #[test]
    fn mapped_ipv4_matches_embedded(octets in any::<[u8; 4]>()) {
        let v4 = Ipv4Addr::from(octets);
        prop_assert_eq!(is_private(IpAddr::V6(v4.to_ipv6_mapped())), is_private(IpAddr::V4(v4)));
    }

    /// Absolute-form targets can't make the reader panic either.
    #[test]
    fn absolute_uri_authority_total(target in any::<String>()) {
        let _authority = absolute_uri_authority(&target);
    }

    /// A target with a host and an explicit port reports both.
    #[test]
    fn absolute_uri_reports_host_and_port(host in hostname(), port in any::<u16>(), path in "[a-z/]{0,12}") {
        let reported = absolute_uri_authority(&format!("http://{host}:{port}/{path}"));
        prop_assert_eq!(reported, Some((host, port)));
    }
}
