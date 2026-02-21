//! ALICE Cloud Gateway Library
//!
//! Receives ASP packets from edge devices, decrypts, routes to
//! ALICE-DB / ALICE-Cache / ALICE-Sync / ALICE-CDN subsystems.
//!
//! Author: Moroya Sakamoto

pub mod container_bridge;
pub mod device_keys;
pub mod ingest;
pub mod queue_bridge;
pub mod telemetry;

use std::net::SocketAddr;

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
            listen_addr: "0.0.0.0:4433"
                .parse()
                .expect("default listen address is a valid SocketAddr"),
            max_packet_size: 65535,
            db_path: "./alice-gateway-data".to_string(),
            cache_capacity: 100_000,
            master_secret: [0u8; 32],
            world_min: [-100.0, -100.0, -100.0],
            world_max: [100.0, 100.0, 100.0],
        }
    }
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
