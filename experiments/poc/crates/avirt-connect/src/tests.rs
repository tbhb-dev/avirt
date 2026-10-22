//! Unit tests for the destination checks and authority parsing.

use core::net::{IpAddr, Ipv4Addr};
use std::collections::HashSet;
use std::io::Error;

use super::{Target, absolute_uri_authority, destination_allowed, host_addresses, is_host_address, is_private};

/// Parse a test address literal.
///
/// # Panics
///
/// Panics when `text` isn't a valid IP address.
fn ip(text: &str) -> IpAddr {
    let parsed = text.parse();
    assert!(parsed.is_ok(), "{text} is not an IP address");
    parsed.unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
}

/// `is_private` refuses every non-global address.
///
/// # Panics
///
/// Panics when an assertion fails.
#[test]
fn private_destinations() {
    for address in [
        "127.0.0.1",
        "10.1.2.3",
        "172.16.0.1",
        "172.31.255.255",
        "192.168.64.1",
        "169.254.1.1",
        "100.64.0.1",
        "100.127.255.255",
        "0.0.0.0",
        "198.18.0.1",
        "198.19.255.255",
        "192.0.0.8",
        "192.0.2.1",
        "198.51.100.7",
        "203.0.113.9",
        "224.0.0.1",
        "239.255.255.255",
        "240.0.0.1",
        "255.255.255.255",
        "192.88.99.1",
        "::ffff:0:808:808",
        "2001:2::1",
        "2001:10::1",
        "2001:2f::1",
        "3fff::1",
        "5f00::1",
        "::1",
        "::",
        "::127.0.0.1",
        "::1.2.3.4",
        "fd00::1",
        "fe80::1",
        "fec0::1",
        "ff02::1",
        "::ffff:127.0.0.1",
        "::ffff:192.168.64.1",
        "64:ff9b::7f00:1",
        "64:ff9b::808:808",
        "64:ff9b:1::1",
        "100::1",
        "2001::1",
        "2001:db8::1",
        "2002:7f00:1::1",
        "2002:c0a8:4001::1",
    ] {
        assert!(is_private(ip(address)), "{address} should be refused");
    }
}

/// `is_private` accepts addresses on the global internet.
///
/// # Panics
///
/// Panics when `is_private` flags a global address.
#[test]
fn global_destinations() {
    for address in [
        "1.1.1.1",
        "8.8.8.8",
        "172.32.0.1",
        "100.128.0.1",
        "198.20.0.1",
        "192.0.3.1",
        "223.255.255.255",
        "2606:4700::1111",
        "2606:4700:4700::1111",
        "2a00:1450:4001::1",
        "::ffff:1.1.1.1",
        "2002:0101:0101::1",
        "64:ff9a::1",
        "2001:4860:4860::8888",
    ] {
        assert!(!is_private(ip(address)), "{address} should be allowed");
    }
}

/// The host's own addresses match, including IPv4-mapped forms.
///
/// # Panics
///
/// Panics when an assertion fails.
#[test]
fn the_hosts_own_addresses_are_recognised() {
    let host: HashSet<IpAddr> = ["203.0.113.50", "2606:4700::5"].into_iter().map(ip).collect();
    assert!(is_host_address(ip("203.0.113.50"), &host));
    assert!(is_host_address(ip("::ffff:203.0.113.50"), &host));
    assert!(is_host_address(ip("2606:4700::5"), &host));
    assert!(!is_host_address(ip("1.1.1.1"), &host));
    // Whatever this machine has, its loopback is among it.
    assert!(is_host_address(ip("127.0.0.1"), &host_addresses().unwrap_or_default()));
}

/// Range edges fall on the right side of the check.
///
/// # Panics
///
/// Panics when an assertion fails.
#[test]
fn range_boundaries_and_mapped_host_addresses() {
    for (address, private) in [
        ("9.255.255.255", false),
        ("10.0.0.0", true),
        ("10.255.255.255", true),
        ("11.0.0.0", false),
        ("100.63.255.255", false),
        ("100.64.0.0", true),
        ("100.127.255.255", true),
        ("100.128.0.0", false),
        ("fbff::1", false),
        ("fc00::1", true),
        ("fdff::1", true),
        ("fe00::1", false),
        ("fe7f::1", false),
        ("fe80::1", true),
        ("febf::1", true),
        ("fec0::1", true),
    ] {
        assert_eq!(is_private(ip(address)), private, "{address}");
    }
    let mut own = HashSet::new();
    assert!(!is_private(ip("8.8.8.8")));
    assert!(!is_host_address(ip("8.8.8.8"), &own));
    own.insert(ip("8.8.8.8"));
    assert!(is_host_address(ip("8.8.8.8"), &own));
    assert!(is_host_address(ip("::ffff:8.8.8.8"), &own));
}

/// Valid authorities parse and `Target::parse` rejects malformed ones.
///
/// # Panics
///
/// Panics when an assertion fails.
#[test]
fn connect_authorities() {
    let parse = |authority| Target::parse(authority);
    assert_eq!(parse("Example.COM.:443"), Some(Target { host: "example.com".into(), port: 443 }));
    assert_eq!(parse("[2606:4700::1]:443"), Some(Target { host: "2606:4700::1".into(), port: 443 }));
    assert_eq!(parse("1.1.1.1:80"), Some(Target { host: "1.1.1.1".into(), port: 80 }));
    for authority in [
        "example.com",
        "example.com:+443",
        "example.com:0443",
        "example.com:00",
        "example.com:65536",
        "example.com:0",
        "example.com:70000",
        ":443",
        "a b:443",
        "[::1:443",
        "exa/mple.com:443",
        "2130706433:443",
        "0177.0.0.1:443",
        "0x7f000001:443",
        "127.1:443",
    ] {
        assert_eq!(parse(authority), None, "{authority}");
    }
}

/// The destination check refuses when it can't be sure.
///
/// # Panics
///
/// Panics when an assertion fails.
#[test]
fn destination_check_fails_closed() {
    let global = ip("93.184.216.34");
    assert!(!destination_allowed(&[global], &Err::<HashSet<IpAddr>, _>(Error::other("unavailable"))));
    assert!(!destination_allowed(&[], &Ok(HashSet::new())));
    assert!(destination_allowed(&[global], &Ok(HashSet::new())));
    assert!(!destination_allowed(&[global], &Ok(HashSet::from([global]))));
    assert!(Target::parse("example.com:443").is_some());
}

/// Absolute-form targets yield a lowercase host and a port.
///
/// # Panics
///
/// Panics when an assertion fails.
#[test]
fn absolute_uri() {
    assert_eq!(absolute_uri_authority("http://Example.com/x?y"), Some(("example.com".into(), 80)));
    assert_eq!(absolute_uri_authority("http://h:8080/"), Some(("h".into(), 8080)));
    assert_eq!(absolute_uri_authority("/x"), None);
}
