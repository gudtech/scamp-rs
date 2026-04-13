//! Network interface resolution and bus_info — Perl Config.pm:59-112.
//!
//! Resolves `if:ethN` syntax in config values to IPv4 addresses,
//! auto-detects private IPs, and provides the bus_info struct used
//! for service binding and multicast configuration.

use std::collections::HashMap;
use std::net::Ipv4Addr;

use crate::config::Config;

/// Resolved network configuration — Perl Config.pm bus_info().
pub struct BusInfo {
    /// Addresses for service listener binding
    pub service_addrs: Vec<Ipv4Addr>,
    /// Addresses for multicast discovery
    pub discovery_addrs: Vec<Ipv4Addr>,
    /// Multicast group address
    pub group: Ipv4Addr,
    /// Discovery port
    pub port: u16,
}

impl BusInfo {
    /// Build BusInfo from config — Perl Config.pm:59-112.
    pub fn from_config(config: &Config) -> Self {
        let interfaces = get_interface_addrs();
        let default_ip = find_default_ip(&interfaces);

        let service_addrs =
            resolve_addr_list(config.get::<String>("bus.address"), &interfaces, default_ip);
        let discovery_addrs = resolve_addr_list(
            config.get::<String>("discovery.address"),
            &interfaces,
            default_ip,
        );
        let group: Ipv4Addr = config
            .get::<String>("discovery.multicast_address")
            .and_then(|r| r.ok())
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| Ipv4Addr::new(239, 63, 248, 106));
        let port: u16 = config
            .get::<u16>("discovery.port")
            .and_then(|r| r.ok())
            .unwrap_or(5555);

        BusInfo {
            service_addrs,
            discovery_addrs,
            group,
            port,
        }
    }

    /// Primary service address for binding — Perl Server.pm:34.
    pub fn service_addr(&self) -> Ipv4Addr {
        self.service_addrs
            .first()
            .copied()
            .unwrap_or(Ipv4Addr::UNSPECIFIED)
    }
}

/// Resolve a config address value: handles `if:ethN` syntax and raw IPs.
/// Perl Config.pm:78-100 (_parse_iflist).
fn resolve_addr_list(
    config_val: Option<Result<String, impl std::fmt::Debug>>,
    interfaces: &HashMap<String, Vec<Ipv4Addr>>,
    default_ip: Option<Ipv4Addr>,
) -> Vec<Ipv4Addr> {
    let raw = match config_val {
        Some(Ok(s)) => s,
        _ => {
            // No config → use default private IP
            return default_ip.into_iter().collect();
        }
    };

    let mut addrs = Vec::new();
    for part in raw.split(',') {
        let part = part.trim();
        if let Some(iface) = part.strip_prefix("if:") {
            // Perl Config.pm:89-93 — resolve interface name
            if let Some(ips) = interfaces.get(iface) {
                addrs.extend(ips);
            } else {
                log::warn!("Interface '{}' not found", iface);
            }
        } else if let Ok(ip) = part.parse::<Ipv4Addr>() {
            addrs.push(ip);
        } else {
            log::warn!("Cannot parse address: {}", part);
        }
    }

    if addrs.is_empty() {
        default_ip.into_iter().collect()
    } else {
        addrs
    }
}

/// Find the default private IP — Perl Config.pm:66-76 (_build_interface_info).
/// Prefers 10.x.x.x, then 192.168.x.x.
fn find_default_ip(interfaces: &HashMap<String, Vec<Ipv4Addr>>) -> Option<Ipv4Addr> {
    let mut all_ips: Vec<Ipv4Addr> = interfaces
        .values()
        .flatten()
        .filter(|ip| !ip.is_loopback())
        .copied()
        .collect();

    // Sort: 10.x first, then 192.168.x, then others
    all_ips.sort_by_key(|ip| {
        let octets = ip.octets();
        match octets[0] {
            10 => 0,
            192 if octets[1] == 168 => 1,
            172 if (16..=31).contains(&octets[1]) => 2,
            _ => 3,
        }
    });

    all_ips.into_iter().next()
}

/// Enumerate network interfaces and their IPv4 addresses.
/// Uses libc getifaddrs — equivalent to Perl's _build_interface_info.
fn get_interface_addrs() -> HashMap<String, Vec<Ipv4Addr>> {
    let mut result: HashMap<String, Vec<Ipv4Addr>> = HashMap::new();

    unsafe {
        let mut ifaddrs: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut ifaddrs) != 0 {
            log::warn!("getifaddrs failed: {}", std::io::Error::last_os_error());
            return result;
        }

        let mut current = ifaddrs;
        while !current.is_null() {
            let ifa = &*current;
            if !ifa.ifa_addr.is_null()
                && (*ifa.ifa_addr).sa_family == libc::AF_INET as libc::sa_family_t
            {
                let name = std::ffi::CStr::from_ptr(ifa.ifa_name)
                    .to_string_lossy()
                    .into_owned();
                let sockaddr = ifa.ifa_addr as *const libc::sockaddr_in;
                let ip = Ipv4Addr::from(u32::from_be((*sockaddr).sin_addr.s_addr));
                result.entry(name).or_default().push(ip);
            }
            current = ifa.ifa_next;
        }
        libc::freeifaddrs(ifaddrs);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_interface_addrs_has_loopback() {
        let addrs = get_interface_addrs();
        // Every system has a loopback interface
        let has_loopback = addrs.values().flatten().any(|ip| ip.is_loopback());
        assert!(has_loopback, "Should find loopback interface");
    }

    #[test]
    fn test_find_default_ip_prefers_private() {
        let mut interfaces = HashMap::new();
        interfaces.insert("lo".to_string(), vec![Ipv4Addr::new(127, 0, 0, 1)]);
        interfaces.insert("eth0".to_string(), vec![Ipv4Addr::new(10, 0, 0, 5)]);
        interfaces.insert("eth1".to_string(), vec![Ipv4Addr::new(192, 168, 1, 100)]);

        let default = find_default_ip(&interfaces);
        assert_eq!(default, Some(Ipv4Addr::new(10, 0, 0, 5)));
    }

    #[test]
    fn test_resolve_if_syntax() {
        let mut interfaces = HashMap::new();
        interfaces.insert("eth0".to_string(), vec![Ipv4Addr::new(10, 0, 0, 5)]);

        let addrs = resolve_addr_list(
            Some(Ok::<_, anyhow::Error>("if:eth0".to_string())),
            &interfaces,
            None,
        );
        assert_eq!(addrs, vec![Ipv4Addr::new(10, 0, 0, 5)]);
    }

    #[test]
    fn test_resolve_raw_ip() {
        let interfaces = HashMap::new();
        let addrs = resolve_addr_list(
            Some(Ok::<_, anyhow::Error>("192.168.1.50".to_string())),
            &interfaces,
            None,
        );
        assert_eq!(addrs, vec![Ipv4Addr::new(192, 168, 1, 50)]);
    }
}
