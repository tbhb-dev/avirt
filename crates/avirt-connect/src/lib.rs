//! Parsing and destination checks for the proxy.

use core::hash::BuildHasher;
use core::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::collections::HashSet;
use std::io::Result as IoResult;

use nix::ifaddrs::getifaddrs;

/// A CONNECT authority: a lowercase hostname or IP literal without a trailing dot, and a port.
#[derive(Debug, PartialEq, Eq)]
pub struct Target {
    /// The lowercase hostname or IP literal. It never has brackets or a trailing dot.
    pub host: String,
    /// The port, from 1 to 65535.
    pub port: u16,
}

impl Target {
    /// Parse `authority` into a target, or return `None` when it isn't a valid CONNECT authority.
    #[must_use]
    pub fn parse(authority: &str) -> Option<Self> {
        let (name, digits) = authority.rsplit_once(':')?;
        if !(1..=5).contains(&digits.len())
            || digits.starts_with('0')
            || !digits.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        let port: u16 = digits.parse().ok().filter(|value| *value != 0)?;
        let host = if let Some(inner) = name.strip_prefix('[').and_then(|inner_host| inner_host.strip_suffix(']')) {
            inner.parse::<Ipv6Addr>().ok()?;
            inner.to_owned()
        } else {
            let trimmed = name.strip_suffix('.').unwrap_or(name);
            let numeric =
                trimmed.rsplit('.').next().is_some_and(|last| last.starts_with(|ch: char| ch.is_ascii_digit()))
                    || trimmed.to_ascii_lowercase().starts_with("0x");
            let valid = !trimmed.is_empty()
                && trimmed.len() <= 253
                && trimmed.split('.').all(|label| {
                    !label.is_empty()
                        && label.len() <= 63
                        && label.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
                });
            // Whatever looks like an IPv4 address must be one in canonical dotted-quad form, so the
            // log and any policy see `127.0.0.1` and not `2130706433`, `0177.0.0.1`, `0x7f000001`
            // or `127.1`, which the system resolver would also accept.
            if !valid || (numeric && trimmed.parse::<Ipv4Addr>().is_err()) {
                return None;
            }
            trimmed.to_owned()
        };
        Some(Self { host: host.to_ascii_lowercase(), port })
    }
}

/// Whether `ip` is anything other than a global unicast address.
///
/// For IPv4 that covers loopback, private, link-local, CGNAT, benchmarking, documentation,
/// multicast, reserved, and broadcast ranges. For IPv6 it covers
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

/// Whether an IPv4 address lies in a non-global range.
fn v4_special(ip: Ipv4Addr) -> bool {
    let [first, second, third, _] = ip.octets();
    first == 0 // "this" network
        || first == 10
        || first == 127
        || (first == 100 && (64..128).contains(&second)) // CGNAT
        || (first == 169 && second == 254)
        || (first == 172 && (16..32).contains(&second))
        || (first == 192 && second == 0 && third == 0) // IETF protocol assignments
        || (first == 192 && second == 0 && third == 2) // documentation
        || (first == 192 && second == 88 && third == 99) // 6to4 relay anycast
        || (first == 192 && second == 168)
        || (first == 198 && (second == 18 || second == 19)) // benchmarking
        || (first == 198 && second == 51 && third == 100) // documentation
        || (first == 203 && second == 0 && third == 113) // documentation
        || first >= 224 // multicast, reserved, and broadcast
}

/// Whether an IPv6 address lies in a non-global range, or embeds a private IPv4 address.
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
///
/// # Errors
///
/// Returns the operating system error when it can't read the interface list.
pub fn host_addresses() -> IoResult<HashSet<IpAddr>> {
    let interfaces = getifaddrs()?;
    Ok(interfaces
        .filter_map(|interface| interface.address)
        .filter_map(|address| {
            address
                .as_sockaddr_in()
                .map(|v4| IpAddr::V4(v4.ip()))
                .or_else(|| address.as_sockaddr_in6().map(|v6| IpAddr::V6(v6.ip())))
        })
        .collect())
}

/// Check every resolved address. Refuse empty answers and unavailable host interfaces.
#[must_use]
pub fn destination_allowed<S>(addrs: &[IpAddr], own: &IoResult<HashSet<IpAddr, S>>) -> bool
where
    S: BuildHasher,
{
    let Ok(interfaces) = own else { return false };
    !addrs.is_empty() && addrs.iter().all(|ip| !is_private(*ip) && !is_host_address(*ip, interfaces))
}

/// Whether the host itself owns `ip` (looking through an IPv4-mapped address).
#[must_use]
pub fn is_host_address<S>(ip: IpAddr, host: &HashSet<IpAddr, S>) -> bool
where
    S: BuildHasher,
{
    let plain = match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        IpAddr::V4(_) => ip,
    };
    host.contains(&plain)
}

/// Host and port of an absolute-form request target such as `http://example.com/x`, for logging.
#[must_use]
pub fn absolute_uri_authority(target: &str) -> Option<(String, u16)> {
    let rest = target.strip_prefix("http://").or_else(|| target.strip_prefix("https://"))?;
    let authority = rest.split(['/', '?']).next()?;
    match authority.rsplit_once(':') {
        Some((host, port)) => Some((host.to_ascii_lowercase(), port.parse().ok()?)),
        None => Some((authority.to_ascii_lowercase(), 80)),
    }
}

#[cfg(test)]
mod tests;
