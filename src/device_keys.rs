//! Device Key Management
//!
//! Derives per-device stream encryption keys from a master secret
//! using BLAKE3 key derivation. Pattern: `derive_stream_key("device:{id}", master)`.
//!
//! Author: Moroya Sakamoto

use alice_crypto::{derive_key, Key};
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;

/// Device key store
///
/// Manages per-device encryption keys derived from a master secret.
/// Keys are cached after first derivation for O(1) subsequent lookups.
pub struct DeviceKeyStore {
    /// Master secret for key derivation
    master_secret: [u8; 32],
    /// Cached derived keys: device_id → Key
    key_cache: parking_lot::Mutex<HashMap<u64, Key>>,
}

impl DeviceKeyStore {
    /// Create a new device key store with the given master secret
    pub fn new(master_secret: [u8; 32]) -> Self {
        Self {
            master_secret,
            key_cache: parking_lot::Mutex::new(HashMap::new()),
        }
    }

    /// Derive (or retrieve cached) encryption key for a device
    ///
    /// Uses BLAKE3 KDF: `derive_key("alice-gw:device:{id}", master_secret)`
    pub fn derive_device_key(&self, device_id: u64) -> Key {
        let mut cache = self.key_cache.lock();

        if let Some(key) = cache.get(&device_id) {
            return key.clone();
        }

        let mut context = String::with_capacity(32);
        let _ = write!(context, "alice-gw:device:{}", device_id);
        let derived = derive_key(&context, &self.master_secret);
        let key = Key::from_bytes(derived);
        cache.insert(device_id, key.clone());
        key
    }

    /// Revoke a device's key (removes from cache, forcing re-derivation)
    pub fn revoke_device(&self, device_id: u64) {
        self.key_cache.lock().remove(&device_id);
    }

    /// Number of cached keys
    pub fn cached_key_count(&self) -> usize {
        self.key_cache.lock().len()
    }

    /// Clear all cached keys
    pub fn clear_cache(&self) {
        self.key_cache.lock().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_derivation_deterministic() {
        let store = DeviceKeyStore::new([1u8; 32]);

        let key1 = store.derive_device_key(42);
        let key2 = store.derive_device_key(42);

        // Same device, same master → same key
        assert_eq!(key1.as_bytes(), key2.as_bytes());
    }

    #[test]
    fn test_different_devices_different_keys() {
        let store = DeviceKeyStore::new([1u8; 32]);

        let key_a = store.derive_device_key(1);
        let key_b = store.derive_device_key(2);

        // Different devices → different keys
        assert_ne!(key_a.as_bytes(), key_b.as_bytes());
    }

    #[test]
    fn test_different_masters_different_keys() {
        let store_a = DeviceKeyStore::new([1u8; 32]);
        let store_b = DeviceKeyStore::new([2u8; 32]);

        let key_a = store_a.derive_device_key(1);
        let key_b = store_b.derive_device_key(1);

        // Same device, different master → different keys
        assert_ne!(key_a.as_bytes(), key_b.as_bytes());
    }

    #[test]
    fn test_key_caching() {
        let store = DeviceKeyStore::new([1u8; 32]);

        assert_eq!(store.cached_key_count(), 0);

        store.derive_device_key(1);
        assert_eq!(store.cached_key_count(), 1);

        store.derive_device_key(2);
        assert_eq!(store.cached_key_count(), 2);

        // Re-derive same key (from cache)
        store.derive_device_key(1);
        assert_eq!(store.cached_key_count(), 2);
    }

    #[test]
    fn test_revoke_device() {
        let store = DeviceKeyStore::new([1u8; 32]);

        store.derive_device_key(1);
        store.derive_device_key(2);
        assert_eq!(store.cached_key_count(), 2);

        store.revoke_device(1);
        assert_eq!(store.cached_key_count(), 1);

        // Can still derive (just not cached)
        let key = store.derive_device_key(1);
        assert_eq!(store.cached_key_count(), 2);
    }

    #[test]
    fn test_clear_cache() {
        let store = DeviceKeyStore::new([1u8; 32]);

        for i in 0..10 {
            store.derive_device_key(i);
        }
        assert_eq!(store.cached_key_count(), 10);

        store.clear_cache();
        assert_eq!(store.cached_key_count(), 0);
    }

    #[test]
    fn test_revoke_nonexistent_device() {
        let store = DeviceKeyStore::new([1u8; 32]);
        // Revoking a device that was never derived should not panic
        store.revoke_device(999);
        assert_eq!(store.cached_key_count(), 0);
    }

    #[test]
    fn test_key_stability_after_revoke_and_rederive() {
        let store = DeviceKeyStore::new([1u8; 32]);

        let key_before = store.derive_device_key(42);
        store.revoke_device(42);
        let key_after = store.derive_device_key(42);

        // Re-derived key must be identical (deterministic KDF)
        assert_eq!(key_before.as_bytes(), key_after.as_bytes());
    }

    #[test]
    fn test_many_devices_unique_keys() {
        let store = DeviceKeyStore::new([0xAB; 32]);

        let keys: Vec<_> = (0..100).map(|i| store.derive_device_key(i)).collect();

        // All 100 keys must be unique
        for i in 0..keys.len() {
            for j in (i + 1)..keys.len() {
                assert_ne!(
                    keys[i].as_bytes(),
                    keys[j].as_bytes(),
                    "Device {} and {} produced same key",
                    i,
                    j
                );
            }
        }
        assert_eq!(store.cached_key_count(), 100);
    }

    #[test]
    fn test_zero_device_id_key_derivation() {
        let store = DeviceKeyStore::new([1u8; 32]);
        // Device ID 0 should still produce a valid key
        let key = store.derive_device_key(0);
        assert!(!key.as_bytes().iter().all(|&b| b == 0));
    }

    #[test]
    fn test_max_device_id_key_derivation() {
        let store = DeviceKeyStore::new([1u8; 32]);
        // u64::MAX should not cause overflow or panic
        let key = store.derive_device_key(u64::MAX);
        assert!(!key.as_bytes().iter().all(|&b| b == 0));
        assert_eq!(store.cached_key_count(), 1);
    }

    #[test]
    fn test_clear_then_rederive() {
        let store = DeviceKeyStore::new([0x55; 32]);
        let key1 = store.derive_device_key(7);
        store.clear_cache();
        assert_eq!(store.cached_key_count(), 0);
        let key2 = store.derive_device_key(7);
        assert_eq!(key1.as_bytes(), key2.as_bytes());
        assert_eq!(store.cached_key_count(), 1);
    }
}
