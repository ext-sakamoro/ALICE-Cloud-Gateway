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
}
