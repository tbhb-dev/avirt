//! Parsing and destination checks for the proxy.

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Everything that isn't a global unicast address: loopback, private, link-local, CGNAT,
/// benchmarking, documentation, multicast, reserved, and broadcast ranges for IPv4. For IPv6 it covers
/// loopback, unspecified, IPv4-compatible, unique-local, link-local, site-local, multicast,
/// discard, documentation, benchmarking, ORCHID, `ORCHIDv2`, segment-routing SIDs,
/// IPv4-translated, Teredo, NAT64 and, through their embedded IPv4 address, IPv4-mapped
/// and 6to4. A destination in one of these is the host or its neighbors, not the
/// internet.
#[must_use]
pub fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4_special(v4),
        IpAddr::V6(v6) => v6_special(v6),
    }
}

fn v4_special(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    a == 0 // "this" network
        || a == 10
        || a == 127
        || (a == 100 && (64..128).contains(&b)) // CGNAT
        || (a == 169 && b == 254)
        || (a == 172 && (16..32).contains(&b))
        || (a == 192 && b == 0 && c == 0) // IETF protocol assignments
        || (a == 192 && b == 0 && c == 2) // documentation
        || (a == 192 && b == 88 && c == 99) // 6to4 relay anycast
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19)) // benchmarking
        || (a == 198 && b == 51 && c == 100) // documentation
        || (a == 203 && b == 0 && c == 113) // documentation
        || a >= 224 // multicast, reserved, and broadcast
}

fn v6_special(ip: Ipv6Addr) -> bool {
    let seg = ip.segments();
    if let Some(v4) = ip.to_ipv4_mapped() {
        return v4_special(v4);
    }
    if seg[..6] == [0; 6] {
        return true; // ::/96, which holds ::, ::1 and the IPv4-compatible addresses
    }
    if (seg[..4] == [0; 4] && seg[4] == 0xffff && seg[5] == 0)
        || (seg[0] == 0x2001 && ((seg[1] == 2 && seg[2] == 0) || (0x0010..=0x002f).contains(&seg[1])))
        || (seg[0] == 0x3fff && seg[1] & 0xf000 == 0)
        || seg[0] == 0x5f00
    {
        return true;
    }
    if seg[0] == 0x0064 && seg[1] == 0xff9b && (seg[2..6] == [0; 4] || seg[2] == 1) {
        return true; // NAT64: 64:ff9b::/96 and 64:ff9b:1::/48
    }
    if seg[0] == 0x0100 && seg[1..4] == [0; 3] {
        return true; // discard-only 100::/64
    }
    if seg[0] == 0x2001 && (seg[1] == 0 || seg[1] == 0x0db8) {
        return true; // Teredo 2001::/32 and documentation 2001:db8::/32
    }
    if seg[0] == 0x2002 {
        // 6to4: the IPv4 address it tunnels to is in the next 32 bits.
        let [o1, o2] = seg[1].to_be_bytes();
        let [o3, o4] = seg[2].to_be_bytes();
        return v4_special(Ipv4Addr::new(o1, o2, o3, o4));
    }
    seg[0] & 0xfe00 == 0xfc00 // fc00::/7 unique local
        || seg[0] & 0xffc0 == 0xfe80 // fe80::/10 link-local
        || seg[0] & 0xffc0 == 0xfec0 // fec0::/10 site-local
        || seg[0] & 0xff00 == 0xff00 // ff00::/8 multicast
}

/// Every address assigned to one of the host's interfaces, global ones included: a service
/// listening on the wildcard address is reachable through any of them.
pub fn host_addresses() -> std::io::Result<HashSet<IpAddr>> {
    let interfaces = nix::ifaddrs::getifaddrs()?;
    Ok(interfaces
        .filter_map(|i| i.address)
        .filter_map(|a| {
            a.as_sockaddr_in()
                .map(|v4| IpAddr::V4(v4.ip()))
                .or_else(|| a.as_sockaddr_in6().map(|v6| IpAddr::V6(v6.ip())))
        })
        .collect())
}

/// Check every resolved address. Refuse empty answers and unavailable host interfaces.
#[must_use]
pub fn destination_allowed<S: std::hash::BuildHasher>(
    addrs: &[IpAddr],
    own: &std::io::Result<HashSet<IpAddr, S>>,
) -> bool {
    let Ok(own) = own else { return false };
    !addrs.is_empty() && addrs.iter().all(|ip| !is_private(*ip) && !is_host_address(*ip, own))
}

/// Whether the host itself owns `ip` (looking through an IPv4-mapped address).
#[must_use]
pub fn is_host_address<S: std::hash::BuildHasher>(ip: IpAddr, host: &HashSet<IpAddr, S>) -> bool {
    let plain = match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        IpAddr::V4(_) => ip,
    };
    host.contains(&plain)
}

/// A CONNECT authority: a lowercase hostname or IP literal without a trailing dot, and a port.
#[derive(Debug, PartialEq, Eq)]
pub struct Target {
    pub host: String,
    pub port: u16,
}

impl Target {
    #[must_use]
    pub fn parse(authority: &str) -> Option<Self> {
        let (host, port) = authority.rsplit_once(':')?;
        if !(1..=5).contains(&port.len()) || port.starts_with('0') || !port.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let port: u16 = port.parse().ok().filter(|p| *p != 0)?;
        let host = if let Some(inner) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
            inner.parse::<Ipv6Addr>().ok()?;
            inner.to_owned()
        } else {
            let host = host.strip_suffix('.').unwrap_or(host);
            let numeric = host.rsplit('.').next().is_some_and(|last| last.starts_with(|c: char| c.is_ascii_digit()))
                || host.to_ascii_lowercase().starts_with("0x");
            let valid = !host.is_empty()
                && host.len() <= 253
                && host.split('.').all(|label| {
                    !label.is_empty()
                        && label.len() <= 63
                        && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                });
            // Whatever looks like an IPv4 address must be one in canonical dotted-quad form, so the
            // log and any policy see `127.0.0.1` and not `2130706433`, `0177.0.0.1`, `0x7f000001`
            // or `127.1`, which the system resolver would also accept.
            if !valid || (numeric && host.parse::<Ipv4Addr>().is_err()) {
                return None;
            }
            host.to_owned()
        };
        Some(Self { host: host.to_ascii_lowercase(), port })
    }
}

/// Host and port of an absolute-form request target such as `http://example.com/x`, for logging.
#[must_use]
pub fn absolute_uri_authority(target: &str) -> Option<(String, u16)> {
    let rest = target.strip_prefix("http://").or_else(|| target.strip_prefix("https://"))?;
    let authority = rest.split(['/', '?']).next()?;
    match authority.rsplit_once(':') {
        Some((h, p)) => Some((h.to_ascii_lowercase(), p.parse().ok()?)),
        None => Some((authority.to_ascii_lowercase(), 80)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn private_destinations() {
        for p in [
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
            assert!(is_private(ip(p)), "{p} should be refused");
        }
        for p in [
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
            assert!(!is_private(ip(p)), "{p} should be allowed");
        }
    }

    #[test]
    fn the_hosts_own_addresses_are_recognised() {
        let host: HashSet<IpAddr> = ["203.0.113.50", "2606:4700::5"].into_iter().map(ip).collect();
        assert!(is_host_address(ip("203.0.113.50"), &host));
        assert!(is_host_address(ip("::ffff:203.0.113.50"), &host));
        assert!(is_host_address(ip("2606:4700::5"), &host));
        assert!(!is_host_address(ip("1.1.1.1"), &host));
        // Whatever this machine has, its loopback is among it.
        assert!(is_host_address(ip("127.0.0.1"), &host_addresses().unwrap()));
    }

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

    #[test]
    fn connect_authorities() {
        let t = |a| Target::parse(a);
        assert_eq!(t("Example.COM.:443"), Some(Target { host: "example.com".into(), port: 443 }));
        assert_eq!(t("[2606:4700::1]:443"), Some(Target { host: "2606:4700::1".into(), port: 443 }));
        assert_eq!(t("1.1.1.1:80"), Some(Target { host: "1.1.1.1".into(), port: 80 }));
        for bad in [
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
            assert_eq!(t(bad), None, "{bad}");
        }
    }

    #[test]
    fn destination_check_fails_closed() {
        let global = ip("93.184.216.34");
        let failed: std::io::Result<HashSet<IpAddr>> = Err(std::io::Error::other("x"));
        assert!(!destination_allowed(&[global], &failed));
        assert!(!destination_allowed(&[], &Ok(HashSet::new())));
        assert!(destination_allowed(&[global], &Ok(HashSet::new())));
        let own: std::io::Result<HashSet<IpAddr>> = Ok([global].into_iter().collect());
        assert!(!destination_allowed(&[global], &own));
        assert!(Target::parse("example.com:443").is_some());
    }

    #[test]
    fn absolute_uri() {
        assert_eq!(absolute_uri_authority("http://Example.com/x?y"), Some(("example.com".into(), 80)));
        assert_eq!(absolute_uri_authority("http://h:8080/"), Some(("h".into(), 8080)));
        assert_eq!(absolute_uri_authority("/x"), None);
    }
}
