//! Which addresses the crawler may connect to.
//!
//! Every URL the crawler fetches came from somebody else: a source list entry, a `<link>` in a
//! page, a manifest, a redirect. Without this check, one hostile entry turns the scheduled CI job
//! into a request forwarder for whatever sits on the runner's network, starting with the cloud
//! metadata service. Only globally routable unicast addresses are allowed. Everything else is
//! refused, including addresses that are merely reserved: a mod site does not live there.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressPolicy {
    /// What every real crawl uses.
    PublicOnly,
    /// Public addresses plus loopback, for fixture servers in tests and local failure drills.
    /// Private, link-local and metadata ranges stay refused even here.
    AllowLoopback,
}

impl AddressPolicy {
    pub fn permits(self, address: IpAddr) -> bool {
        if is_public(address) {
            return true;
        }
        self == Self::AllowLoopback && address.is_loopback()
    }
}

pub fn is_public(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_public_v4(address),
        IpAddr::V6(address) => is_public_v6(address),
    }
}

const REFUSED_V4: [([u8; 4], u32); 15] = [
    ([0, 0, 0, 0], 8),       // "this network"
    ([10, 0, 0, 0], 8),      // private
    ([100, 64, 0, 0], 10),   // carrier-grade NAT; Alibaba's metadata service lives here
    ([127, 0, 0, 0], 8),     // loopback
    ([169, 254, 0, 0], 16),  // link-local, including 169.254.169.254
    ([172, 16, 0, 0], 12),   // private
    ([192, 0, 0, 0], 24),    // IETF protocol assignments
    ([192, 0, 2, 0], 24),    // documentation
    ([192, 88, 99, 0], 24),  // 6to4 relay anycast
    ([192, 168, 0, 0], 16),  // private
    ([198, 18, 0, 0], 15),   // benchmarking
    ([198, 51, 100, 0], 24), // documentation
    ([203, 0, 113, 0], 24),  // documentation
    ([224, 0, 0, 0], 4),     // multicast
    ([240, 0, 0, 0], 4),     // reserved and broadcast
];

fn in_v4(address: Ipv4Addr, network: [u8; 4], prefix: u32) -> bool {
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    u32::from(address) & mask == u32::from(Ipv4Addr::from(network)) & mask
}

fn is_public_v4(address: Ipv4Addr) -> bool {
    !REFUSED_V4
        .iter()
        .any(|(network, prefix)| in_v4(address, *network, *prefix))
}

// Inside 2000::/3, but not somewhere a mod site lives.
const REFUSED_V6: [([u16; 8], u32); 4] = [
    ([0x2001, 0, 0, 0, 0, 0, 0, 0], 23), // IETF protocol assignments, Teredo
    ([0x2001, 0x0db8, 0, 0, 0, 0, 0, 0], 32), // documentation
    ([0x2002, 0, 0, 0, 0, 0, 0, 0], 16), // 6to4
    ([0x3fff, 0, 0, 0, 0, 0, 0, 0], 20), // documentation
];

fn in_v6(address: Ipv6Addr, network: [u16; 8], prefix: u32) -> bool {
    let mask = if prefix == 0 {
        0
    } else {
        u128::MAX << (128 - prefix)
    };
    u128::from(address) & mask == u128::from(Ipv6Addr::from(network)) & mask
}

fn is_public_v6(address: Ipv6Addr) -> bool {
    // An IPv4 address wearing IPv6 clothes is judged as the IPv4 address it carries.
    if let Some(embedded) = address.to_ipv4_mapped() {
        return is_public_v4(embedded);
    }
    if in_v6(address, [0x64, 0xff9b, 0, 0, 0, 0, 0, 0], 96) {
        let [.., a, b, c, d] = address.octets();
        return is_public_v4(Ipv4Addr::new(a, b, c, d));
    }
    // Global unicast is 2000::/3. Loopback, unspecified, IPv4-compatible, unique local,
    // link-local, site-local and multicast all live outside it.
    if !in_v6(address, [0x2000, 0, 0, 0, 0, 0, 0, 0], 3) {
        return false;
    }
    !REFUSED_V6
        .iter()
        .any(|(network, prefix)| in_v6(address, *network, *prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn public(text: &str) -> bool {
        is_public(text.parse().unwrap())
    }

    #[test]
    fn ordinary_hosts_are_public() {
        for address in [
            "185.199.108.153",
            "140.82.112.3",
            "8.8.8.8",
            "2606:50c0:8000::153",
            "2a01:4f8::1",
        ] {
            assert!(public(address), "{address} refused");
        }
    }

    #[test]
    fn local_private_and_metadata_ranges_are_refused() {
        for address in [
            "127.0.0.1",
            "127.8.8.8",
            "0.0.0.0",
            "10.1.2.3",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "100.100.100.200",
            "224.0.0.1",
            "255.255.255.255",
            "198.18.0.1",
            "::",
            "::1",
            "fe80::1",
            "fc00::1",
            "fd00:ec2::254",
            "ff02::1",
            "::ffff:127.0.0.1",
            "::ffff:169.254.169.254",
            "64:ff9b::a00:1",
            "2001:db8::1",
            "2002:7f00:1::",
            "::127.0.0.1",
        ] {
            assert!(!public(address), "{address} allowed");
        }
    }

    #[test]
    fn mapped_public_addresses_stay_public() {
        assert!(public("::ffff:8.8.8.8"));
        assert!(public("64:ff9b::808:808"));
    }

    #[test]
    fn loopback_drills_never_open_private_ranges() {
        let policy = AddressPolicy::AllowLoopback;
        assert!(policy.permits("127.0.0.1".parse().unwrap()));
        assert!(policy.permits("::1".parse().unwrap()));
        assert!(!policy.permits("10.0.0.1".parse().unwrap()));
        assert!(!policy.permits("169.254.169.254".parse().unwrap()));
        assert!(!AddressPolicy::PublicOnly.permits("127.0.0.1".parse().unwrap()));
    }
}
