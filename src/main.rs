//! ALICE Cloud Gateway
//!
//! Receives ASP packets from edge devices, decrypts, routes to
//! ALICE-DB / ALICE-Cache / ALICE-Sync / ALICE-CDN subsystems.
//!
//! Author: Moroya Sakamoto

mod ingest;
mod device_keys;
mod telemetry;
mod queue_bridge;
mod container_bridge;

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use log::{info, warn, error};

use crate::ingest::IngestPipeline;
use crate::device_keys::DeviceKeyStore;
use crate::telemetry::GatewayTelemetry;

/// Gateway configuration
pub struct GatewayConfig {
    /// UDP listen address (QUIC/ASP)
    pub listen_addr: SocketAddr,
    /// Maximum packet size (bytes)
    pub max_packet_size: usize,
    /// Database storage path
    pub db_path: String,
    /// Cache capacity (entries)
    pub cache_capacity: usize,
    /// Master secret for key derivation
    pub master_secret: [u8; 32],
    /// World bounds for SDF storage
    pub world_min: [f32; 3],
    pub world_max: [f32; 3],
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:4433".parse().unwrap(),
            max_packet_size: 65535,
            db_path: "./alice-gateway-data".to_string(),
            cache_capacity: 100_000,
            master_secret: [0u8; 32],
            world_min: [-100.0, -100.0, -100.0],
            world_max: [100.0, 100.0, 100.0],
        }
    }
}

/// Run the cloud gateway
async fn run_gateway(config: GatewayConfig) -> std::io::Result<()> {
    info!("ALICE Cloud Gateway starting on {}", config.listen_addr);

    let pipeline = Arc::new(
        IngestPipeline::new(
            &config.db_path,
            config.cache_capacity,
            config.master_secret,
            config.world_min,
            config.world_max,
        )?
    );

    let socket = UdpSocket::bind(config.listen_addr).await?;
    info!("Listening on UDP {}", config.listen_addr);

    let mut buf = vec![0u8; config.max_packet_size];

    loop {
        match socket.recv_from(&mut buf).await {
            Ok((len, src)) => {
                let data = buf[..len].to_vec();
                let pipeline = Arc::clone(&pipeline);

                tokio::spawn(async move {
                    match pipeline.process_packet(&data, src) {
                        Ok(stats) => {
                            if stats.is_keyframe {
                                info!(
                                    "Keyframe from {} (device {}): {} bytes, scene v{}",
                                    src, stats.device_id, len, stats.scene_version
                                );
                            }
                        }
                        Err(e) => {
                            warn!("Packet from {} rejected: {}", src, e);
                        }
                    }
                });
            }
            Err(e) => {
                error!("UDP recv error: {}", e);
            }
        }
    }
}

fn main() -> std::io::Result<()> {
    env_logger::init();

    let config = GatewayConfig::default();

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(run_gateway(config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_config_default() {
        let config = GatewayConfig::default();
        assert_eq!(config.listen_addr.port(), 4433);
        assert_eq!(config.max_packet_size, 65535);
        assert_eq!(config.cache_capacity, 100_000);
    }
}
