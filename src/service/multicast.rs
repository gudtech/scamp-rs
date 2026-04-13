//! UDP multicast announcement sender.
//!
//! Sends zlib-compressed announcement packets on a configurable interval.
//! Matches Perl Discovery::Announcer.pm BUILD/_start/shutdown.

use anyhow::Result;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use log;
use socket2::{Domain, Protocol, Socket, Type};
use std::io::Write;
use std::net::{Ipv4Addr, SocketAddrV4};
use tokio::sync::watch;

/// Default multicast group — Perl Config.pm:110
pub const DEFAULT_MULTICAST_GROUP: &str = "239.63.248.106";

/// Default multicast port — Perl Config.pm:109
pub const DEFAULT_MULTICAST_PORT: u16 = 5555;

/// Default announce interval in seconds — Perl Announcer.pm:40
pub const DEFAULT_INTERVAL_SECS: u32 = 5;

/// Shutdown rounds — Perl Announcer.pm:93
const SHUTDOWN_ROUNDS: u32 = 10;

/// Configuration for multicast announcing.
pub struct MulticastConfig {
    pub group: Ipv4Addr,
    pub port: u16,
    pub interface: Ipv4Addr,
    pub interval_secs: u32,
}

impl MulticastConfig {
    pub fn new(interface: Ipv4Addr) -> Self {
        MulticastConfig {
            group: DEFAULT_MULTICAST_GROUP.parse().unwrap(),
            port: DEFAULT_MULTICAST_PORT,
            interface,
            interval_secs: DEFAULT_INTERVAL_SECS,
        }
    }

    /// Build from scamp config, falling back to defaults.
    pub fn from_config(config: &crate::config::Config, interface: Ipv4Addr) -> Self {
        let group = config
            .get::<String>("discovery.multicast_address")
            .and_then(|r| r.ok())
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| DEFAULT_MULTICAST_GROUP.parse().unwrap());

        let port = config
            .get::<u16>("discovery.port")
            .and_then(|r| r.ok())
            .unwrap_or(DEFAULT_MULTICAST_PORT);

        MulticastConfig {
            group,
            port,
            interface,
            interval_secs: DEFAULT_INTERVAL_SECS,
        }
    }
}

/// Compress data with zlib level 9. Perl Announcer.pm:203
pub fn zlib_compress(data: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(data)?;
    Ok(encoder.finish()?)
}

/// Create a UDP socket configured for multicast sending.
/// Perl Announcer.pm:62-69
fn create_multicast_socket(config: &MulticastConfig) -> Result<std::net::UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;

    // Bind to interface — Perl: LocalHost => $to
    let bind_addr = SocketAddrV4::new(config.interface, 0);
    socket.bind(&bind_addr.into())?;

    // Set multicast interface — Perl: $sock->mcast_if($to)
    socket.set_multicast_if_v4(&config.interface)?;

    socket.set_nonblocking(false)?;

    log::info!(
        "Multicast socket bound on {} → {}:{}",
        config.interface,
        config.group,
        config.port
    );
    Ok(socket.into())
}

/// Run the multicast announcement loop.
///
/// Sends compressed announcements every `interval_secs` until `shutdown_rx` fires,
/// then sends 10 rounds of weight=0 announcements at 1s intervals.
///
/// `build_packet_fn` is called each iteration to get the current (uncompressed) packet.
/// Perl Announcer.pm:78-94
pub async fn run_announcer<F>(
    config: MulticastConfig,
    mut build_packet: F,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<()>
where
    F: FnMut(bool) -> Result<Vec<u8>> + Send,
{
    let socket = create_multicast_socket(&config)?;
    let dest = SocketAddrV4::new(config.group, config.port);

    // Normal announcing loop — Perl Announcer.pm:85-91
    loop {
        let packet = build_packet(true)?;
        let compressed = zlib_compress(&packet)?;

        if let Err(e) = socket.send_to(&compressed, dest) {
            log::error!("Announce send failed: {}", e);
        } else {
            log::debug!("Sent announcement ({} bytes compressed)", compressed.len());
        }

        // Wait for interval or shutdown signal
        let sleep = tokio::time::sleep(tokio::time::Duration::from_secs(
            config.interval_secs as u64,
        ));
        tokio::select! {
            _ = sleep => {},
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    break;
                }
            }
        }
    }

    // Shutdown announcing: weight=0, 10 rounds at 1s — Perl Announcer.pm:82-94,97-101
    log::info!("Sending shutdown announcements (weight=0, {} rounds)", SHUTDOWN_ROUNDS);
    for round in 1..=SHUTDOWN_ROUNDS {
        let packet = build_packet(false)?;
        let compressed = zlib_compress(&packet)?;

        if let Err(e) = socket.send_to(&compressed, dest) {
            log::error!("Shutdown announce {} failed: {}", round, e);
        } else {
            log::debug!("Shutdown announce {}/{}", round, SHUTDOWN_ROUNDS);
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }

    log::info!("Shutdown announcing complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zlib_compress_decompress() {
        use flate2::read::ZlibDecoder;
        use std::io::Read;

        // Use a large enough payload that compression actually helps
        let original = "hello world, this is a test of zlib compression for scamp. ".repeat(20);
        let compressed = zlib_compress(original.as_bytes()).unwrap();
        assert!(compressed.len() < original.len());

        let mut decoder = ZlibDecoder::new(&compressed[..]);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).unwrap();
        assert_eq!(decompressed, original.as_bytes());
    }

    #[test]
    fn test_default_config() {
        let cfg = MulticastConfig::new(Ipv4Addr::new(10, 0, 0, 1));
        assert_eq!(cfg.group, Ipv4Addr::new(239, 63, 248, 106));
        assert_eq!(cfg.port, 5555);
        assert_eq!(cfg.interval_secs, 5);
    }
}
