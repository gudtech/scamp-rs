//! Multicast observer: receives announcements from other services (D24).
//! Perl Observer.pm: joins multicast group, decompresses packets, injects into registry.

use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;

use super::packet::AnnouncementPacket;
use super::service_registry::ServiceRegistry;
use crate::auth::authorized_services::AuthorizedServices;
use crate::config::Config;

/// Configuration for the multicast observer.
pub struct ObserverConfig {
    pub group: Ipv4Addr,
    pub port: u16,
    pub interface: Ipv4Addr,
}

impl ObserverConfig {
    pub fn from_config(config: &Config, interface: Ipv4Addr) -> Self {
        let group: Ipv4Addr = config
            .get::<String>("discovery.multicast_address")
            .and_then(|r| r.ok())
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| Ipv4Addr::new(239, 63, 248, 106));
        let port: u16 = config.get::<u16>("discovery.port").and_then(|r| r.ok()).unwrap_or(5555);
        ObserverConfig { group, port, interface }
    }
}

/// Run the multicast observer loop — Perl Observer.pm:18-61.
/// Listens for zlib-compressed announcement packets and injects them into the registry.
pub async fn run_observer(
    obs_config: ObserverConfig,
    registry: Arc<RwLock<ServiceRegistry>>,
    auth: Arc<AuthorizedServices>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    let socket = create_observer_socket(&obs_config)?;
    let socket = tokio::net::UdpSocket::from_std(socket)?;
    let mut buf = vec![0u8; 65536];

    log::info!(
        "Observer listening on {}:{} (interface {})",
        obs_config.group,
        obs_config.port,
        obs_config.interface
    );

    loop {
        tokio::select! {
            result = socket.recv_from(&mut buf) => {
                let (len, _src) = result?;
                if let Err(e) = process_packet(&buf[..len], &registry, &auth).await {
                    log::debug!("Observer: failed to process packet: {}", e);
                }
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() { break; }
            }
        }
    }

    log::info!("Observer shutting down");
    Ok(())
}

/// Process a single received multicast packet.
async fn process_packet(data: &[u8], registry: &Arc<RwLock<ServiceRegistry>>, auth: &AuthorizedServices) -> Result<()> {
    // Perl Observer.pm:48 — skip 'R' or 'D' prefix if present
    let data = match data.first() {
        Some(b'R') | Some(b'D') => &data[1..],
        _ => data,
    };

    // Decompress zlib — Perl Observer.pm:50
    let decompressed = zlib_decompress(data)?;
    let text = std::str::from_utf8(&decompressed)?;

    // Parse the announcement packet
    let packet = AnnouncementPacket::parse(text)?;

    // Inject into registry (holds write lock briefly)
    let mut reg = registry.write().await;
    reg.inject_packet(packet, auth);

    Ok(())
}

/// Decompress zlib data — inverse of multicast.rs zlib_compress.
fn zlib_decompress(data: &[u8]) -> Result<Vec<u8>> {
    use flate2::read::ZlibDecoder;
    use std::io::Read;
    let mut decoder = ZlibDecoder::new(data);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out)?;
    Ok(out)
}

/// Create a UDP socket for multicast receiving — Perl Observer.pm:18-36.
fn create_observer_socket(config: &ObserverConfig) -> Result<std::net::UdpSocket> {
    use socket2::{Domain, Protocol, SockAddr, Socket, Type};

    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;

    // macOS requires SO_REUSEPORT for multicast
    #[cfg(target_os = "macos")]
    socket.set_reuse_port(true)?;

    // Bind to the multicast group address — Perl: LocalHost => $group
    let bind_addr = SockAddr::from(SocketAddrV4::new(config.group, config.port));
    socket.bind(&bind_addr)?;

    // Join multicast group on specified interface
    socket.join_multicast_v4(&config.group, &config.interface)?;
    socket.set_nonblocking(true)?;

    Ok(socket.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zlib_decompress_roundtrip() {
        let original = b"test announcement data for compression roundtrip".repeat(5);
        let compressed = crate::service::multicast::zlib_compress(&original).unwrap();
        let decompressed = zlib_decompress(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }
}
