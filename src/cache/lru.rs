use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};

use super::metrics::CacheMetrics;

struct CacheEntry<V> {
    value: V,
    inserted_at: Instant,
    last_accessed: Instant,
}

pub struct LruCache<K, V> {
    store: HashMap<K, CacheEntry<V>>,
    max_size: usize,
    ttl: Duration,
    metrics: CacheMetrics,
}

impl<K: Eq + Hash + Clone, V: Clone> LruCache<K, V> {
    pub fn new(max_size: usize, ttl: Duration) -> Self {
        Self {
            store: HashMap::with_capacity(max_size),
            max_size,
            ttl,
            metrics: CacheMetrics::new(),
        }
    }

    pub fn get(&mut self, key: &K) -> Option<V> {
        let now = Instant::now();
        if let Some(entry) = self.store.get_mut(key) {
            if now.duration_since(entry.inserted_at) > self.ttl {
                self.store.remove(key);
                self.metrics.record_eviction();
                self.metrics.record_miss();
                return None;
            }
            entry.last_accessed = now;
            self.metrics.record_hit();
            return Some(entry.value.clone());
        }
        self.metrics.record_miss();
        None
    }

    pub fn insert(&mut self, key: K, value: V) {
        if self.store.len() >= self.max_size && !self.store.contains_key(&key) {
            self.evict_lru();
        }
        let now = Instant::now();
        self.store.insert(
            key,
            CacheEntry {
                value,
                inserted_at: now,
                last_accessed: now,
            },
        );
        self.metrics.set_size(self.store.len());
    }

    pub fn invalidate(&mut self, key: &K) -> bool {
        let removed = self.store.remove(key).is_some();
        if removed {
            self.metrics.record_eviction();
            self.metrics.set_size(self.store.len());
        }
        removed
    }

    pub fn clear(&mut self) {
        self.store.clear();
        self.metrics.set_size(0);
    }

    pub fn contains(&self, key: &K) -> bool {
        self.store
            .get(key)
            .is_some_and(|e| Instant::now().duration_since(e.inserted_at) <= self.ttl)
    }

    pub fn len(&self) -> usize {
        self.store.len()
    }

    pub fn is_empty(&self) -> bool {
        self.store.is_empty()
    }

    pub fn metrics(&self) -> &CacheMetrics {
        &self.metrics
    }

    fn evict_lru(&mut self) {
        if let Some(key) = self
            .store
            .iter()
            .min_by_key(|(_, e)| e.last_accessed)
            .map(|(k, _)| k.clone())
        {
            self.store.remove(&key);
            self.metrics.record_eviction();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get() {
        let mut cache = LruCache::new(10, Duration::from_secs(60));
        cache.insert("key1", "value1");
        assert_eq!(cache.get(&"key1"), Some("value1"));
    }

    #[test]
    fn test_miss() {
        let mut cache: LruCache<&str, &str> = LruCache::new(10, Duration::from_secs(60));
        assert_eq!(cache.get(&"missing"), None);
    }

    #[test]
    fn test_eviction_at_capacity() {
        let mut cache = LruCache::new(2, Duration::from_secs(60));
        cache.insert("a", 1);
        cache.insert("b", 2);
        cache.insert("c", 3);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_invalidate() {
        let mut cache = LruCache::new(10, Duration::from_secs(60));
        cache.insert("key", "val");
        assert!(cache.invalidate(&"key"));
        assert!(!cache.invalidate(&"key"));
        assert!(cache.is_empty());
    }

    #[test]
    fn test_clear() {
        let mut cache = LruCache::new(10, Duration::from_secs(60));
        cache.insert("a", 1);
        cache.insert("b", 2);
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_contains() {
        let mut cache = LruCache::new(10, Duration::from_secs(60));
        cache.insert("key", "val");
        assert!(cache.contains(&"key"));
        assert!(!cache.contains(&"other"));
    }

    #[test]
    fn test_metrics_tracking() {
        let mut cache = LruCache::new(10, Duration::from_secs(60));
        cache.insert("key", "val");
        cache.get(&"key"); // hit
        cache.get(&"missing"); // miss
        assert_eq!(cache.metrics().hits(), 1);
        assert_eq!(cache.metrics().misses(), 1);
    }
}
