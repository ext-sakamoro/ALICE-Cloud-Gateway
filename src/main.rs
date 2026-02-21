//! ALICE Cloud Gateway Binary
//!
//! Author: Moroya Sakamoto

use log::{error, info, warn};
use std::sync::Arc;
use tokio::net::UdpSocket;

use alice_cloud_gateway::{ingest::IngestPipeline, GatewayConfig};

/// Run the cloud gateway
async fn run_gateway(config: GatewayConfig) -> std::io::Result<()> {
    info!("ALICE Cloud Gateway starting on {}", config.listen_addr);

    let pipeline = Arc::new(IngestPipeline::new(
        &config.db_path,
        config.cache_capacity,
        config.master_secret,
        config.world_min,
        config.world_max,
    )?);

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
