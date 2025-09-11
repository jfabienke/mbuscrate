//! # Compact Frame LRU Cache for wM-Bus
//!
//! This module implements an LRU (Least Recently Used) cache for compact frames
//! according to OMS specification. Compact frames (CI=0x79) use a 2-byte signature
//! to reference cached device information, reducing transmission overhead.
//!
//! ## Cache Features
//!
//! - LRU eviction policy for memory-bounded operation
//! - Configurable size (256-1024 entries recommended)
//! - Fast O(1) lookup and insertion
//! - Thread-safe operation with interior mutability
//! - Statistics tracking for monitoring
//!
//! ## Usage
//!
//! ```rust
//! use mbus_rs::wmbus::compact_cache::CompactFrameCache;
//!
//! let mut cache = CompactFrameCache::new(256);
//! 
//! // Store device info with signature
//! cache.insert(0xABCD, device_info);
//!
//! // Retrieve device info by signature
//! if let Some(info) = cache.get(0xABCD) {
//!     // Use cached device information
//! }
//! ```

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Device information cached for compact frames
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedDeviceInfo {
    /// Manufacturer identifier (2 bytes)
    pub manufacturer_id: u16,
    /// Device address (4 bytes)
    pub device_address: u32,
    /// Device version
    pub version: u8,
    /// Device type/medium
    pub device_type: u8,
    /// Last seen timestamp (not serialized)
    #[serde(skip, default = "Instant::now")]
    pub last_seen: Instant,
    /// Last seen timestamp as Unix timestamp (for serialization)
    #[serde(default)]
    pub last_seen_unix: u64,
    /// Number of times accessed
    pub access_count: u64,
}

/// LRU cache for compact frame device information
#[derive(Debug)]
pub struct CompactFrameCache {
    /// Internal cache storage with LRU tracking
    inner: Arc<Mutex<CacheInner>>,
}

#[derive(Debug)]
struct CacheInner {
    /// Device info indexed by signature
    devices: HashMap<u16, CachedDeviceInfo>,
    /// LRU queue tracking access order (most recent at back)
    lru_queue: VecDeque<u16>,
    /// Maximum cache size
    max_size: usize,
    /// Cache statistics
    stats: CacheStats,
}

/// Statistics for cache monitoring
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct CacheStats {
    /// Total insertions
    pub insertions: u64,
    /// Total lookups
    pub lookups: u64,
    /// Cache hits
    pub hits: u64,
    /// Cache misses
    pub misses: u64,
    /// Evictions due to size limit
    pub evictions: u64,
}

/// Serializable cache data structure
#[derive(Debug, Serialize, Deserialize)]
struct CacheData {
    /// Device information map
    devices: HashMap<u16, CachedDeviceInfo>,
    /// Cache statistics
    stats: CacheStats,
    /// Maximum cache size
    max_size: usize,
}

impl CompactFrameCache {
    /// Create a new cache with specified maximum size
    ///
    /// # Arguments
    ///
    /// * `max_size` - Maximum number of cached devices (256-1024 recommended)
    pub fn new(max_size: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(CacheInner {
                devices: HashMap::with_capacity(max_size),
                lru_queue: VecDeque::with_capacity(max_size),
                max_size,
                stats: CacheStats::default(),
            })),
        }
    }
    
    /// Insert or update device information in cache
    ///
    /// # Arguments
    ///
    /// * `signature` - 2-byte compact frame signature
    /// * `info` - Device information to cache
    pub fn insert(&self, signature: u16, info: CachedDeviceInfo) {
        let mut inner = self.inner.lock().unwrap();
        inner.stats.insertions += 1;
        
        // Remove from LRU queue if already present
        if let Some(pos) = inner.lru_queue.iter().position(|&s| s == signature) {
            inner.lru_queue.remove(pos);
        }
        
        // Check if eviction needed
        if inner.devices.len() >= inner.max_size && !inner.devices.contains_key(&signature) {
            // Evict least recently used
            if let Some(lru_signature) = inner.lru_queue.pop_front() {
                inner.devices.remove(&lru_signature);
                inner.stats.evictions += 1;
            }
        }
        
        // Insert/update device info
        inner.devices.insert(signature, info);
        // Add to back of LRU queue (most recent)
        inner.lru_queue.push_back(signature);
    }
    
    /// Retrieve device information by signature
    ///
    /// Updates LRU order and access statistics.
    ///
    /// # Arguments
    ///
    /// * `signature` - 2-byte compact frame signature
    ///
    /// # Returns
    ///
    /// * `Some(info)` - Cached device information if found
    /// * `None` - If signature not in cache
    pub fn get(&self, signature: u16) -> Option<CachedDeviceInfo> {
        let mut inner = self.inner.lock().unwrap();
        inner.stats.lookups += 1;
        
        // Check if device exists and update it
        if inner.devices.contains_key(&signature) {
            inner.stats.hits += 1;
            
            // Update device info
            let info = inner.devices.get_mut(&signature).unwrap();
            info.access_count += 1;
            info.last_seen = Instant::now();
            let result = info.clone();
            
            // Update LRU order
            if let Some(pos) = inner.lru_queue.iter().position(|&s| s == signature) {
                inner.lru_queue.remove(pos);
            }
            inner.lru_queue.push_back(signature);
            
            Some(result)
        } else {
            inner.stats.misses += 1;
            None
        }
    }
    
    /// Remove device from cache
    ///
    /// # Arguments
    ///
    /// * `signature` - 2-byte compact frame signature
    ///
    /// # Returns
    ///
    /// * `true` if device was removed
    /// * `false` if signature not found
    pub fn remove(&self, signature: u16) -> bool {
        let mut inner = self.inner.lock().unwrap();
        
        // Remove from LRU queue
        if let Some(pos) = inner.lru_queue.iter().position(|&s| s == signature) {
            inner.lru_queue.remove(pos);
        }
        
        inner.devices.remove(&signature).is_some()
    }
    
    /// Clear all cached entries
    pub fn clear(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.devices.clear();
        inner.lru_queue.clear();
    }
    
    /// Get current cache size
    pub fn size(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner.devices.len()
    }
    
    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let inner = self.inner.lock().unwrap();
        inner.stats
    }
    
    /// Calculate cache hit rate
    pub fn hit_rate(&self) -> f64 {
        let stats = self.stats();
        if stats.lookups > 0 {
            stats.hits as f64 / stats.lookups as f64
        } else {
            0.0
        }
    }
    
    /// Remove entries older than specified duration
    ///
    /// # Arguments
    ///
    /// * `max_age` - Maximum age for cached entries
    ///
    /// # Returns
    ///
    /// * Number of entries removed
    pub fn remove_stale(&self, max_age: Duration) -> usize {
        let mut inner = self.inner.lock().unwrap();
        let now = Instant::now();
        let mut removed = 0;
        
        // Collect signatures of stale entries
        let stale_signatures: Vec<u16> = inner
            .devices
            .iter()
            .filter(|(_, info)| now.duration_since(info.last_seen) > max_age)
            .map(|(&sig, _)| sig)
            .collect();
        
        // Remove stale entries
        for signature in stale_signatures {
            inner.devices.remove(&signature);
            if let Some(pos) = inner.lru_queue.iter().position(|&s| s == signature) {
                inner.lru_queue.remove(pos);
            }
            removed += 1;
        }
        
        removed
    }
    
    /// Build a CI=0x76 full frame request for a compact frame signature
    ///
    /// When a meter sends a compact frame (CI=0x79) with a signature that's not
    /// in the cache, the receiver can request the full frame using CI=0x76.
    ///
    /// # Arguments
    ///
    /// * `signature` - 2-byte compact frame signature
    /// * `device_address` - Device address (if known, otherwise 0xFF for broadcast)
    ///
    /// # Returns
    ///
    /// * Frame bytes for CI=0x76 request
    pub fn build_full_frame_request(signature: u16, device_address: u8) -> Vec<u8> {
        let mut frame = Vec::new();
        
        // Start byte for short frame
        frame.push(0x10);
        
        // Control field - REQ_UD2 (request user data class 2)
        frame.push(0x7B);
        
        // Address field
        frame.push(device_address);
        
        // CI field - 0x76 for full frame request
        frame.push(0x76);
        
        // Signature (2 bytes, little-endian)
        frame.push((signature & 0xFF) as u8);
        frame.push((signature >> 8) as u8);
        
        // Calculate checksum
        let checksum = frame[1..].iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        frame.push(checksum);
        
        // Stop byte
        frame.push(0x16);
        
        frame
    }
    
    /// Save cache to JSON file
    ///
    /// Persists the cache state to disk for recovery after restart.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to save the cache file
    ///
    /// # Returns
    ///
    /// * `Ok(())` on success
    /// * `Err(e)` on I/O or serialization error
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let inner = self.inner.lock().unwrap();
        
        // Prepare serializable data
        let cache_data = CacheData {
            devices: inner.devices.iter()
                .map(|(&sig, info)| {
                    let mut info_with_unix = info.clone();
                    // Convert Instant to Unix timestamp for serialization
                    info_with_unix.last_seen_unix = info.last_seen.elapsed().as_secs();
                    (sig, info_with_unix)
                })
                .collect(),
            stats: inner.stats,
            max_size: inner.max_size,
        };
        
        let json = serde_json::to_string_pretty(&cache_data)?;
        fs::write(path, json)?;
        Ok(())
    }
    
    /// Load cache from JSON file
    ///
    /// Restores cache state from a previously saved file.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the cache file
    ///
    /// # Returns
    ///
    /// * `Ok(cache)` - Loaded cache instance
    /// * `Err(e)` on I/O or deserialization error
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let json = fs::read_to_string(path)?;
        let cache_data: CacheData = serde_json::from_str(&json)?;
        
        let cache = Self::new(cache_data.max_size);
        let mut inner = cache.inner.lock().unwrap();
        
        // Restore devices with current timestamp
        let now = Instant::now();
        for (sig, mut info) in cache_data.devices {
            // Restore Instant from Unix timestamp
            info.last_seen = now - Duration::from_secs(info.last_seen_unix);
            inner.devices.insert(sig, info);
            inner.lru_queue.push_back(sig);
        }
        
        inner.stats = cache_data.stats;
        
        drop(inner);
        Ok(cache)
    }
    
    /// Create cache from device address (generate signature)
    ///
    /// Generates a 2-byte signature from device address using CRC-like algorithm.
    ///
    /// # Arguments
    ///
    /// * `device_address` - 4-byte device address
    ///
    /// # Returns
    ///
    /// * 2-byte signature for compact frame
    pub fn generate_signature(device_address: u32) -> u16 {
        // Simple CRC-like signature generation
        let bytes = device_address.to_le_bytes();
        let mut sig = 0u16;
        
        for &byte in &bytes {
            sig = sig.wrapping_add(byte as u16);
            sig = (sig << 1) | (sig >> 15); // Rotate left
            sig ^= 0xA5A5; // XOR with pattern
        }
        
        sig
    }
}

impl Default for CompactFrameCache {
    fn default() -> Self {
        Self::new(256) // Default to 256 entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cache_basic_operations() {
        let cache = CompactFrameCache::new(3);
        
        // Insert entries
        let info1 = CachedDeviceInfo {
            manufacturer_id: 0x1234,
            device_address: 0xAABBCCDD,
            version: 1,
            device_type: 0x04,
            last_seen: Instant::now(),
            last_seen_unix: 0,
            access_count: 0,
        };
        
        cache.insert(0x0001, info1.clone());
        assert_eq!(cache.size(), 1);
        
        // Retrieve entry
        let retrieved = cache.get(0x0001).unwrap();
        assert_eq!(retrieved.manufacturer_id, 0x1234);
        assert_eq!(retrieved.access_count, 1); // Incremented on get
        
        // Check stats
        let stats = cache.stats();
        assert_eq!(stats.insertions, 1);
        assert_eq!(stats.lookups, 1);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 0);
    }
    
    #[test]
    fn test_lru_eviction() {
        let cache = CompactFrameCache::new(2); // Small cache for testing
        
        let info = CachedDeviceInfo {
            manufacturer_id: 0x1234,
            device_address: 0,
            version: 1,
            device_type: 0x04,
            last_seen: Instant::now(),
            last_seen_unix: 0,
            access_count: 0,
        };
        
        // Fill cache
        cache.insert(0x0001, info.clone());
        cache.insert(0x0002, info.clone());
        assert_eq!(cache.size(), 2);
        
        // Insert third item should evict first
        cache.insert(0x0003, info.clone());
        assert_eq!(cache.size(), 2);
        
        // First item should be evicted
        assert!(cache.get(0x0001).is_none());
        assert!(cache.get(0x0002).is_some());
        assert!(cache.get(0x0003).is_some());
        
        // Check eviction count
        let stats = cache.stats();
        assert_eq!(stats.evictions, 1);
    }
    
    #[test]
    fn test_lru_order_update() {
        let cache = CompactFrameCache::new(2);
        
        let info = CachedDeviceInfo {
            manufacturer_id: 0x1234,
            device_address: 0,
            version: 1,
            device_type: 0x04,
            last_seen: Instant::now(),
            last_seen_unix: 0,
            access_count: 0,
        };
        
        // Insert two items
        cache.insert(0x0001, info.clone());
        cache.insert(0x0002, info.clone());
        
        // Access first item to make it most recent
        cache.get(0x0001);
        
        // Insert third item should evict second (least recent)
        cache.insert(0x0003, info.clone());
        
        assert!(cache.get(0x0001).is_some()); // Still present
        assert!(cache.get(0x0002).is_none()); // Evicted
        assert!(cache.get(0x0003).is_some()); // New item
    }
    
    #[test]
    fn test_signature_generation() {
        let sig1 = CompactFrameCache::generate_signature(0x12345678);
        let sig2 = CompactFrameCache::generate_signature(0x12345678);
        let sig3 = CompactFrameCache::generate_signature(0x87654321);
        
        // Same input should give same signature
        assert_eq!(sig1, sig2);
        // Different input should give different signature
        assert_ne!(sig1, sig3);
    }
    
    #[test]
    fn test_cache_hit_rate() {
        let cache = CompactFrameCache::new(10);
        
        let info = CachedDeviceInfo {
            manufacturer_id: 0x1234,
            device_address: 0,
            version: 1,
            device_type: 0x04,
            last_seen: Instant::now(),
            last_seen_unix: 0,
            access_count: 0,
        };
        
        cache.insert(0x0001, info);
        
        // 3 hits
        cache.get(0x0001);
        cache.get(0x0001);
        cache.get(0x0001);
        
        // 2 misses
        cache.get(0x0002);
        cache.get(0x0003);
        
        let hit_rate = cache.hit_rate();
        assert!((hit_rate - 0.6).abs() < 0.01); // 3/5 = 0.6
    }
    
    #[test]
    fn test_full_frame_request() {
        // Test building a CI=0x76 full frame request
        let signature = 0xABCD;
        let device_address = 0x42;
        
        let frame = CompactFrameCache::build_full_frame_request(signature, device_address);
        
        // Verify frame structure
        assert_eq!(frame[0], 0x10); // Start byte (short frame)
        assert_eq!(frame[1], 0x7B); // Control field (REQ_UD2)
        assert_eq!(frame[2], 0x42); // Address
        assert_eq!(frame[3], 0x76); // CI field for full frame request
        assert_eq!(frame[4], 0xCD); // Signature low byte
        assert_eq!(frame[5], 0xAB); // Signature high byte
        
        // Verify checksum
        let checksum = frame[1..6].iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        assert_eq!(frame[6], checksum);
        
        assert_eq!(frame[7], 0x16); // Stop byte
        assert_eq!(frame.len(), 8); // Total frame length
    }
    
    #[test]
    fn test_cache_persistence() {
        use std::fs;
        use tempfile::TempDir;
        
        // Create temporary directory for test
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join("cache.json");
        
        // Create and populate cache
        let cache1 = CompactFrameCache::new(5);
        
        let info = CachedDeviceInfo {
            manufacturer_id: 0x5678,
            device_address: 0x11223344,
            version: 2,
            device_type: 0x07,
            last_seen: Instant::now(),
            last_seen_unix: 0,
            access_count: 10,
        };
        
        cache1.insert(0x1234, info.clone());
        cache1.insert(0x5678, info.clone());
        
        // Generate some stats
        cache1.get(0x1234);
        cache1.get(0x9999); // Miss
        
        // Save cache
        cache1.save_to_file(&cache_path).unwrap();
        
        // Verify file exists
        assert!(cache_path.exists());
        
        // Load cache from file
        let cache2 = CompactFrameCache::load_from_file(&cache_path).unwrap();
        
        // Verify loaded cache has same data
        assert_eq!(cache2.size(), 2);
        
        let loaded_info = cache2.get(0x1234).unwrap();
        assert_eq!(loaded_info.manufacturer_id, 0x5678);
        assert_eq!(loaded_info.device_address, 0x11223344);
        assert_eq!(loaded_info.version, 2);
        assert_eq!(loaded_info.device_type, 0x07);
        
        // Stats should be preserved (with updated counts from get operations)
        let stats = cache2.stats();
        assert!(stats.insertions >= 2);
        assert!(stats.lookups >= 3); // Original 2 + 1 from verification
        
        // Clean up
        fs::remove_file(cache_path).ok();
    }
}