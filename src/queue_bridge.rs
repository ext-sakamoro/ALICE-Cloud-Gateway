//! ALICE-Cloud-Gateway × ALICE-Queue bridge
//!
//! Ingest pipeline message routing — ASP packets to processing queues.
//!
//! Author: Moroya Sakamoto

/// Message priority for queue routing
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum MessagePriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Routable message envelope
#[derive(Debug, Clone)]
pub struct GatewayMessage {
    pub source_device_id: u64,
    pub timestamp_ms: u64,
    pub priority: MessagePriority,
    pub payload: Vec<u8>,
    pub routing_key: u32,
}

impl GatewayMessage {
    /// Serialize header (24 bytes) + payload
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(24 + self.payload.len());
        buf.extend_from_slice(&self.source_device_id.to_le_bytes());
        buf.extend_from_slice(&self.timestamp_ms.to_le_bytes());
        buf.push(self.priority as u8);
        buf.extend_from_slice(&self.routing_key.to_le_bytes());
        buf.extend_from_slice(&(self.payload.len() as u16).to_le_bytes());
        buf.push(0); // padding
        buf.extend_from_slice(&self.payload);
        buf
    }

    #[must_use]
    pub fn from_bytes(buf: &[u8]) -> Option<Self> {
        if buf.len() < 24 {
            return None;
        }
        let source_device_id = u64::from_le_bytes(buf[0..8].try_into().ok()?);
        let timestamp_ms = u64::from_le_bytes(buf[8..16].try_into().ok()?);
        let priority = match buf[16] {
            0 => MessagePriority::Low,
            1 => MessagePriority::Normal,
            2 => MessagePriority::High,
            3 => MessagePriority::Critical,
            _ => return None,
        };
        let routing_key = u32::from_le_bytes(buf[17..21].try_into().ok()?);
        let payload_len = u16::from_le_bytes(buf[21..23].try_into().ok()?) as usize;
        if buf.len() < 24 + payload_len {
            return None;
        }
        let payload = buf[24..24 + payload_len].to_vec();
        Some(Self {
            source_device_id,
            timestamp_ms,
            priority,
            payload,
            routing_key,
        })
    }
}

/// Gateway message router
pub struct GatewayRouter {
    pub messages_routed: u64,
    pub bytes_routed: u64,
    queue_count: u32,
}

impl GatewayRouter {
    #[must_use]
    pub fn new(queue_count: u32) -> Self {
        Self {
            messages_routed: 0,
            bytes_routed: 0,
            queue_count: queue_count.max(1),
        }
    }

    /// Determine target queue index for a message
    pub fn route(&mut self, msg: &GatewayMessage) -> u32 {
        self.messages_routed += 1;
        self.bytes_routed += msg.payload.len() as u64;
        // Consistent hash: routing_key mod queue_count
        msg.routing_key % self.queue_count
    }

    /// Route with priority override (critical → queue 0)
    pub fn route_with_priority(&mut self, msg: &GatewayMessage) -> u32 {
        self.messages_routed += 1;
        self.bytes_routed += msg.payload.len() as u64;
        if msg.priority == MessagePriority::Critical {
            0 // Critical messages always go to queue 0
        } else {
            msg.routing_key % self.queue_count
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_msg(key: u32, priority: MessagePriority) -> GatewayMessage {
        GatewayMessage {
            source_device_id: 1,
            timestamp_ms: 1_000_000,
            priority,
            payload: vec![0xAA; 64],
            routing_key: key,
        }
    }

    #[test]
    fn test_message_serialization() {
        let msg = test_msg(42, MessagePriority::Normal);
        let bytes = msg.to_bytes();
        let restored = GatewayMessage::from_bytes(&bytes).unwrap();
        assert_eq!(restored.source_device_id, 1);
        assert_eq!(restored.routing_key, 42);
        assert_eq!(restored.priority, MessagePriority::Normal);
        assert_eq!(restored.payload.len(), 64);
    }

    #[test]
    fn test_routing() {
        let mut router = GatewayRouter::new(4);
        let msg = test_msg(7, MessagePriority::Normal);
        assert_eq!(router.route(&msg), 3); // 7 % 4 = 3
    }

    #[test]
    fn test_priority_routing() {
        let mut router = GatewayRouter::new(4);
        let critical = test_msg(7, MessagePriority::Critical);
        assert_eq!(router.route_with_priority(&critical), 0); // Always queue 0
    }

    #[test]
    fn test_router_counters() {
        let mut router = GatewayRouter::new(2);
        let msg = test_msg(1, MessagePriority::Low);
        router.route(&msg);
        assert_eq!(router.messages_routed, 1);
        assert_eq!(router.bytes_routed, 64);
    }

    #[test]
    fn test_message_too_short() {
        assert!(GatewayMessage::from_bytes(&[0; 10]).is_none());
    }

    #[test]
    fn test_message_exactly_24_bytes_empty_payload() {
        let msg = GatewayMessage {
            source_device_id: 1,
            timestamp_ms: 100,
            priority: MessagePriority::Low,
            payload: vec![],
            routing_key: 0,
        };
        let bytes = msg.to_bytes();
        let restored = GatewayMessage::from_bytes(&bytes).unwrap();
        assert_eq!(restored.source_device_id, 1);
        assert!(restored.payload.is_empty());
    }

    #[test]
    fn test_message_invalid_priority_byte() {
        // Build a minimal 24-byte buffer with invalid priority byte (value 4)
        let mut buf = vec![0u8; 24];
        buf[16] = 4; // Invalid priority
        assert!(GatewayMessage::from_bytes(&buf).is_none());
    }

    #[test]
    fn test_message_payload_too_short_for_declared_len() {
        let msg = GatewayMessage {
            source_device_id: 1,
            timestamp_ms: 100,
            priority: MessagePriority::Normal,
            payload: vec![0xBB; 10],
            routing_key: 5,
        };
        let mut bytes = msg.to_bytes();
        // Truncate payload portion so it's shorter than declared
        bytes.truncate(26);
        assert!(GatewayMessage::from_bytes(&bytes).is_none());
    }

    #[test]
    fn test_routing_distribution() {
        let mut router = GatewayRouter::new(4);
        let mut counts = [0u32; 4];
        for key in 0..100 {
            let msg = test_msg(key, MessagePriority::Normal);
            let idx = router.route(&msg);
            counts[idx as usize] += 1;
        }
        // Each queue should get exactly 25 messages (100/4 = 25)
        assert_eq!(counts, [25, 25, 25, 25]);
    }

    #[test]
    fn test_priority_routing_non_critical() {
        let mut router = GatewayRouter::new(4);
        // Non-critical messages should route by routing_key
        let high = test_msg(7, MessagePriority::High);
        let normal = test_msg(7, MessagePriority::Normal);
        let low = test_msg(7, MessagePriority::Low);
        assert_eq!(router.route_with_priority(&high), 3); // 7 % 4 = 3
        assert_eq!(router.route_with_priority(&normal), 3); // 7 % 4 = 3
        assert_eq!(router.route_with_priority(&low), 3); // 7 % 4 = 3
    }

    #[test]
    fn test_router_new_minimum_queue_count() {
        // queue_count 0 should be clamped to 1
        let mut router = GatewayRouter::new(0);
        let msg = test_msg(42, MessagePriority::Normal);
        let idx = router.route(&msg);
        assert_eq!(idx, 0); // 42 % 1 = 0
    }

    #[test]
    fn test_message_roundtrip_all_priorities() {
        for (prio, byte_val) in [
            (MessagePriority::Low, 0u8),
            (MessagePriority::Normal, 1),
            (MessagePriority::High, 2),
            (MessagePriority::Critical, 3),
        ] {
            let msg = GatewayMessage {
                source_device_id: 0xDEAD,
                timestamp_ms: 42,
                priority: prio,
                payload: vec![1, 2, 3],
                routing_key: 99,
            };
            let bytes = msg.to_bytes();
            // Verify priority byte position
            assert_eq!(bytes[16], byte_val);
            let restored = GatewayMessage::from_bytes(&bytes).unwrap();
            assert_eq!(restored.priority, prio);
        }
    }

    #[test]
    fn test_priority_ordering() {
        assert!(MessagePriority::Low < MessagePriority::Normal);
        assert!(MessagePriority::Normal < MessagePriority::High);
        assert!(MessagePriority::High < MessagePriority::Critical);
    }

    #[test]
    fn test_router_bytes_routed_accumulation() {
        let mut router = GatewayRouter::new(2);
        let msg1 = GatewayMessage {
            source_device_id: 1,
            timestamp_ms: 0,
            priority: MessagePriority::Normal,
            payload: vec![0; 100],
            routing_key: 0,
        };
        let msg2 = GatewayMessage {
            source_device_id: 1,
            timestamp_ms: 0,
            priority: MessagePriority::Normal,
            payload: vec![0; 200],
            routing_key: 0,
        };
        router.route(&msg1);
        router.route(&msg2);
        assert_eq!(router.bytes_routed, 300);
        assert_eq!(router.messages_routed, 2);
    }

    #[test]
    fn test_message_clone() {
        let msg = test_msg(5, MessagePriority::High);
        let cloned = msg.clone();
        assert_eq!(cloned.routing_key, 5);
        assert_eq!(cloned.priority, MessagePriority::High);
    }

    #[test]
    fn test_message_debug() {
        let msg = test_msg(1, MessagePriority::Low);
        let s = format!("{:?}", msg);
        assert!(s.contains("source_device_id"));
    }

    #[test]
    fn test_routing_key_zero() {
        let mut router = GatewayRouter::new(4);
        let msg = test_msg(0, MessagePriority::Normal);
        assert_eq!(router.route(&msg), 0);
    }

    #[test]
    fn test_priority_critical_route_with_priority_counters() {
        let mut router = GatewayRouter::new(4);
        let msg = test_msg(99, MessagePriority::Critical);
        router.route_with_priority(&msg);
        assert_eq!(router.messages_routed, 1);
    }
}
