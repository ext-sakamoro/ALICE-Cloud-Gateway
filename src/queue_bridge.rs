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
}
