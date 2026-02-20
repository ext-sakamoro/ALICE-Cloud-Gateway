//! ASP Packet Ingest Pipeline
//!
//! Receives encrypted ASP packets from edge devices, decrypts,
//! parses, and routes to storage/cache/sync/CDN subsystems.
//!
//! Author: Moroya Sakamoto

use std::fmt::Write as FmtWrite;
use std::net::SocketAddr;

use alice_cache::AliceCache;
use alice_cdn::sdf_cdn_bridge::{SdfCdnRouter, SdfRoutingStats};
use alice_db::sdf_bridge::{MortonCode, SdfStorage};
use alice_sync::cloud_bridge::{CloudSyncHub, SpatialRegion};
use alice_sync::WorldHash;

use parking_lot::Mutex;

use crate::device_keys::DeviceKeyStore;
use crate::queue_bridge::{GatewayMessage, GatewayRouter, MessagePriority};
use crate::telemetry::GatewayTelemetry;

/// Result of processing a single packet
#[derive(Debug)]
pub struct IngestResult {
    /// Device that sent this packet
    pub device_id: u64,
    /// Whether this was a keyframe (I-Packet)
    pub is_keyframe: bool,
    /// Scene version from the packet
    pub scene_version: u32,
    /// Number of recipients for sync broadcast
    pub sync_recipients: usize,
    /// Cache was updated
    pub cached: bool,
    /// CDN edge node assigned (if CDN router is active, for keyframes)
    pub cdn_node: Option<u64>,
    /// Queue index for downstream processing
    pub queue_index: u32,
    /// Processing latency in milliseconds
    pub latency_ms: f64,
}

/// Ingest pipeline error
#[derive(Debug)]
pub enum IngestError {
    /// Unknown device (not registered)
    UnknownDevice(u64),
    /// Decryption failed
    DecryptionFailed(String),
    /// Malformed packet
    MalformedPacket(String),
    /// Storage error
    StorageError(String),
}

impl std::fmt::Display for IngestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownDevice(id) => write!(f, "Unknown device: {}", id),
            Self::DecryptionFailed(msg) => write!(f, "Decryption failed: {}", msg),
            Self::MalformedPacket(msg) => write!(f, "Malformed packet: {}", msg),
            Self::StorageError(msg) => write!(f, "Storage error: {}", msg),
        }
    }
}

impl std::error::Error for IngestError {}

/// ASP Packet Ingest Pipeline
///
/// Routes incoming packets through:
/// 1. Decrypt (ALICE-Crypto)
/// 2. Parse ASP packet (libasp)
/// 3. Store SDF data (ALICE-DB)
/// 4. Cache hot frames (ALICE-Cache)
/// 5. Sync to other devices (ALICE-Sync)
/// 6. Route to CDN edge nodes (ALICE-CDN)
/// 7. Route to processing queues (ALICE-Queue)
/// 8. Record telemetry (ALICE-Analytics)
pub struct IngestPipeline {
    /// Device key management
    key_store: DeviceKeyStore,
    /// SDF spatial storage
    sdf_storage: SdfStorage,
    /// Hot frame cache: (device_id << 32 | scene_version) → packet bytes
    frame_cache: AliceCache<u64, Vec<u8>>,
    /// Multi-device sync hub
    sync_hub: Mutex<CloudSyncHub>,
    /// CDN routing (initialized via `set_cdn_router`)
    cdn_router: Mutex<Option<SdfCdnRouter>>,
    /// Message queue router
    queue_router: Mutex<GatewayRouter>,
    /// Telemetry collection
    telemetry: Mutex<GatewayTelemetry>,
    /// CDN routing stats
    routing_stats: Mutex<SdfRoutingStats>,
    /// World bounds min (stored from constructor for SDF spatial indexing)
    world_min: [f32; 3],
    /// World bounds max
    world_max: [f32; 3],
}

impl IngestPipeline {
    /// Create a new ingest pipeline
    ///
    /// # Arguments
    /// * `db_path` - Directory for SDF spatial storage
    /// * `cache_capacity` - Number of frames to cache
    /// * `master_secret` - 32-byte master secret for device key derivation
    /// * `world_min` - World space minimum bounds for Morton code indexing
    /// * `world_max` - World space maximum bounds for Morton code indexing
    pub fn new(
        db_path: &str,
        cache_capacity: usize,
        master_secret: [u8; 32],
        world_min: [f32; 3],
        world_max: [f32; 3],
    ) -> std::io::Result<Self> {
        let key_store = DeviceKeyStore::new(master_secret);
        let sdf_storage = SdfStorage::open(format!("{}/sdf", db_path), world_min, world_max)?;
        let frame_cache = AliceCache::new(cache_capacity);
        let sync_hub = Mutex::new(CloudSyncHub::new());
        let telemetry = Mutex::new(GatewayTelemetry::new());
        let queue_router = Mutex::new(GatewayRouter::new(4)); // 4 processing queues

        Ok(Self {
            key_store,
            sdf_storage,
            frame_cache,
            sync_hub,
            cdn_router: Mutex::new(None),
            queue_router,
            telemetry,
            routing_stats: Mutex::new(SdfRoutingStats::default()),
            world_min,
            world_max,
        })
    }

    /// Initialize CDN router with edge node topology
    ///
    /// Once set, keyframes will be routed to optimal edge nodes
    /// via Maglev consistent hashing.
    pub fn set_cdn_router(&self, router: SdfCdnRouter) {
        *self.cdn_router.lock() = Some(router);
    }

    /// Process an incoming ASP packet
    ///
    /// Full pipeline: decrypt → parse → store → cache → sync → telemetry
    pub fn process_packet(
        &self,
        raw_data: &[u8],
        source: SocketAddr,
    ) -> Result<IngestResult, IngestError> {
        let start = std::time::Instant::now();

        // Step 1: Extract device ID from packet header (first 8 bytes)
        if raw_data.len() < 12 {
            return Err(IngestError::MalformedPacket(
                "Packet too short (< 12 bytes)".to_string(),
            ));
        }

        let device_id = u64::from_le_bytes(raw_data[..8].try_into().unwrap());
        let scene_version = u32::from_le_bytes(raw_data[8..12].try_into().unwrap());

        // Step 2: Decrypt packet payload
        let stream_key = self.key_store.derive_device_key(device_id);
        let payload = &raw_data[12..];
        let decrypted = alice_crypto::open(&stream_key, payload)
            .map_err(|e| IngestError::DecryptionFailed(format!("{:?}", e)))?;

        // Step 3: Determine packet type (first byte of decrypted payload)
        let is_keyframe = !decrypted.is_empty() && decrypted[0] == 0x01;

        // Step 4: Store SDF data in ALICE-DB and route to CDN
        let mut cdn_node = None;
        if is_keyframe && decrypted.len() > 1 {
            // Extract center coordinates from payload (bytes 1-12)
            let (cx, cy, cz) = if decrypted.len() >= 13 {
                let cx = f32::from_le_bytes(decrypted[1..5].try_into().unwrap());
                let cy = f32::from_le_bytes(decrypted[5..9].try_into().unwrap());
                let cz = f32::from_le_bytes(decrypted[9..13].try_into().unwrap());
                (cx, cy, cz)
            } else {
                (0.0, 0.0, 0.0)
            };

            let morton = MortonCode::from_world(cx, cy, cz, self.world_min, self.world_max);

            // Extract SDF value from payload (first f32 after position data)
            let sdf_value = if decrypted.len() >= 17 {
                f32::from_le_bytes(decrypted[13..17].try_into().unwrap())
            } else {
                0.0
            };

            self.sdf_storage
                .store_keyframe(morton, scene_version, sdf_value)
                .map_err(|e| IngestError::StorageError(e.to_string()))?;

            // Route keyframe to CDN edge node (Maglev consistent hash)
            cdn_node = self
                .cdn_router
                .lock()
                .as_ref()
                .and_then(|router: &SdfCdnRouter| router.route_sdf_request(morton.0 as u32));
        }

        // Step 5: Cache frame and track hit/miss
        let cache_key = (device_id << 32) | (scene_version as u64);
        let cache_hit = self.frame_cache.get(&cache_key).is_some();
        self.frame_cache.put(cache_key, decrypted);

        // Step 6: Sync to other devices
        let sync_recipients = {
            let mut hub = self.sync_hub.lock();
            let mut device_name = String::with_capacity(24);
            let _ = write!(device_name, "device-{}", device_id);
            hub.register_device(device_id, device_name);
            hub.heartbeat(device_id, start.elapsed().as_millis() as u64);

            let update_region = SpatialRegion::new([-10.0, -10.0, -10.0], [10.0, 10.0, 10.0]);
            hub.process_device_update(
                device_id,
                scene_version,
                WorldHash(scene_version as u64),
                update_region,
            )
        };

        // Step 7: Route to processing queue
        let queue_index = {
            let msg = GatewayMessage {
                source_device_id: device_id,
                timestamp_ms: start.elapsed().as_millis() as u64,
                priority: if is_keyframe {
                    MessagePriority::High
                } else {
                    MessagePriority::Normal
                },
                payload: Vec::new(),
                routing_key: scene_version,
            };
            self.queue_router.lock().route_with_priority(&msg)
        };

        // Step 8: Record telemetry and routing stats
        let raw_len = raw_data.len();
        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
        {
            let mut tel = self.telemetry.lock();
            tel.record_packet(device_id, raw_len, latency_ms, is_keyframe);
        }
        {
            let mut stats = self.routing_stats.lock();
            stats.total_requests += 1;
            if cache_hit {
                stats.cache_hits += 1;
            }
        }

        Ok(IngestResult {
            device_id,
            is_keyframe,
            scene_version,
            sync_recipients: sync_recipients.len(),
            cached: true,
            cdn_node,
            queue_index,
            latency_ms,
        })
    }

    /// Query SDF data for a spatial region
    pub fn query_spatial_region(
        &self,
        region_min: [f32; 3],
        region_max: [f32; 3],
    ) -> Result<Vec<(MortonCode, f32)>, IngestError> {
        self.sdf_storage
            .query_spatial_region(region_min, region_max)
            .map_err(|e| IngestError::StorageError(e.to_string()))
    }

    /// Get cached frame for a device + scene version
    pub fn get_cached_frame(&self, device_id: u64, scene_version: u32) -> Option<Vec<u8>> {
        let cache_key = (device_id << 32) | (scene_version as u64);
        self.frame_cache.get(&cache_key)
    }

    /// Get telemetry snapshot
    pub fn telemetry_snapshot(&self) -> GatewayTelemetry {
        self.telemetry.lock().clone()
    }

    /// Get routing stats
    pub fn routing_stats(&self) -> SdfRoutingStats {
        self.routing_stats.lock().clone()
    }

    /// Get connected device count
    pub fn connected_device_count(&self) -> usize {
        self.sync_hub.lock().connected_devices().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_test_pipeline() -> IngestPipeline {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test-gw");
        IngestPipeline::new(
            db_path.to_str().unwrap(),
            1000,
            [42u8; 32],
            [-100.0, -100.0, -100.0],
            [100.0, 100.0, 100.0],
        )
        .unwrap()
    }

    fn make_test_packet(device_id: u64, scene_version: u32, is_keyframe: bool) -> Vec<u8> {
        let key_store = DeviceKeyStore::new([42u8; 32]);
        let stream_key = key_store.derive_device_key(device_id);

        // Build payload: type byte + position + sdf data
        let mut payload = Vec::new();
        payload.push(if is_keyframe { 0x01 } else { 0x02 });
        // Center position (0, 0, 0)
        payload.extend_from_slice(&0.0f32.to_le_bytes());
        payload.extend_from_slice(&0.0f32.to_le_bytes());
        payload.extend_from_slice(&0.0f32.to_le_bytes());
        // Some SDF data
        payload.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);

        // Encrypt
        let sealed = alice_crypto::seal(&stream_key, &payload).unwrap();

        // Build full packet: device_id (8B) + scene_version (4B) + sealed payload
        let mut packet = Vec::new();
        packet.extend_from_slice(&device_id.to_le_bytes());
        packet.extend_from_slice(&scene_version.to_le_bytes());
        packet.extend_from_slice(&sealed);
        packet
    }

    #[test]
    fn test_ingest_keyframe() {
        let pipeline = make_test_pipeline();
        let packet = make_test_packet(1, 1, true);
        let src: SocketAddr = "127.0.0.1:5000".parse().unwrap();

        let result = pipeline.process_packet(&packet, src).unwrap();
        assert_eq!(result.device_id, 1);
        assert!(result.is_keyframe);
        assert_eq!(result.scene_version, 1);
        assert!(result.cached);
        // No CDN router configured, so cdn_node should be None
        assert!(result.cdn_node.is_none());
        assert!(result.latency_ms >= 0.0);
    }

    #[test]
    fn test_ingest_delta() {
        let pipeline = make_test_pipeline();
        let packet = make_test_packet(2, 5, false);
        let src: SocketAddr = "127.0.0.1:5001".parse().unwrap();

        let result = pipeline.process_packet(&packet, src).unwrap();
        assert_eq!(result.device_id, 2);
        assert!(!result.is_keyframe);
        assert_eq!(result.scene_version, 5);
        // Delta packets never get CDN routing
        assert!(result.cdn_node.is_none());
    }

    #[test]
    fn test_ingest_cache_hit() {
        let pipeline = make_test_pipeline();
        let packet = make_test_packet(1, 1, true);
        let src: SocketAddr = "127.0.0.1:5000".parse().unwrap();

        pipeline.process_packet(&packet, src).unwrap();

        // Should be cached now
        let cached = pipeline.get_cached_frame(1, 1);
        assert!(cached.is_some());
    }

    #[test]
    fn test_ingest_malformed_packet() {
        let pipeline = make_test_pipeline();
        let short_packet = vec![0u8; 5]; // Too short
        let src: SocketAddr = "127.0.0.1:5000".parse().unwrap();

        let result = pipeline.process_packet(&short_packet, src);
        assert!(result.is_err());
    }

    #[test]
    fn test_telemetry_recording() {
        let pipeline = make_test_pipeline();
        let packet = make_test_packet(1, 1, true);
        let src: SocketAddr = "127.0.0.1:5000".parse().unwrap();

        pipeline.process_packet(&packet, src).unwrap();

        let tel = pipeline.telemetry_snapshot();
        assert_eq!(tel.total_packets, 1);
        assert_eq!(tel.keyframe_count, 1);
    }

    #[test]
    fn test_multi_device_sync() {
        let pipeline = make_test_pipeline();
        let src: SocketAddr = "127.0.0.1:5000".parse().unwrap();

        // Register two devices by sending packets
        let pkt1 = make_test_packet(1, 1, true);
        let pkt2 = make_test_packet(2, 1, true);

        pipeline.process_packet(&pkt1, src).unwrap();
        pipeline.process_packet(&pkt2, src).unwrap();

        assert_eq!(pipeline.connected_device_count(), 2);
    }

    #[test]
    fn test_queue_routing() {
        let pipeline = make_test_pipeline();
        let src: SocketAddr = "127.0.0.1:5000".parse().unwrap();

        let pkt_kf = make_test_packet(1, 1, true);
        let pkt_delta = make_test_packet(1, 2, false);

        let r1 = pipeline.process_packet(&pkt_kf, src).unwrap();
        let r2 = pipeline.process_packet(&pkt_delta, src).unwrap();

        // Keyframes use High priority → route_with_priority
        // Queue index is deterministic based on scene_version % 4
        assert!(r1.queue_index < 4);
        assert!(r2.queue_index < 4);
    }

    #[test]
    fn test_routing_stats_cache_tracking() {
        let pipeline = make_test_pipeline();
        let src: SocketAddr = "127.0.0.1:5000".parse().unwrap();

        // First packet: cache miss (new entry)
        let pkt1 = make_test_packet(1, 1, true);
        pipeline.process_packet(&pkt1, src).unwrap();

        let stats1 = pipeline.routing_stats();
        assert_eq!(stats1.total_requests, 1);
        assert_eq!(stats1.cache_hits, 0); // First time = miss

        // Same device + scene_version: cache hit
        let pkt2 = make_test_packet(1, 1, true);
        pipeline.process_packet(&pkt2, src).unwrap();

        let stats2 = pipeline.routing_stats();
        assert_eq!(stats2.total_requests, 2);
        assert_eq!(stats2.cache_hits, 1); // Second time = hit
    }

    #[test]
    fn test_sdf_value_extraction() {
        let pipeline = make_test_pipeline();
        let src: SocketAddr = "127.0.0.1:5000".parse().unwrap();

        // Keyframe packet stores SDF data; test that it doesn't error
        let pkt = make_test_packet(1, 1, true);
        let result = pipeline.process_packet(&pkt, src).unwrap();
        assert!(result.is_keyframe);

        // Verify spatial query returns data
        let data = pipeline.query_spatial_region([-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]);
        assert!(data.is_ok());
    }
}
