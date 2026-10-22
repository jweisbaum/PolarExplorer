//! What has been downloaded this session, kept in memory (spec.md 3.4, 13).
//!
//! Several boats of one race need the same reanalysis blocks, so the blocks a
//! fetch downloads are kept for reuse. The app uses an unbounded store;
//! callers can also request a bounded store. **Nothing here reaches the disk**: a project keeps
//! only the values interpolated at its samples (invariant 3), and quitting
//! the application forgets the rest.
//!
//! What is kept, per `(archive, shard key)`:
//! - its shard index, or that the archive has no such shard;
//! - each inner chunk downloaded, compressed exactly as served;
//! - each inner chunk's decoded value bytes, sharing the same byte limit.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use bytes::Bytes;

/// Which part of a chunk an entry holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Part {
    /// A shard's index (or that the shard is missing).
    Head,
    /// One inner chunk, compressed, by number.
    Block(u32),
    /// One inner chunk, decoded value bytes.
    Decoded(u32),
}

/// What an entry holds.
#[derive(Debug, Clone)]
pub enum Held {
    /// The archive has no such object: no data, never an error.
    Missing,
    /// Compressed bytes exactly as served (a shard index or an inner chunk).
    Compressed(Bytes),
    /// Validated decoded bytes, retained without expanding float16 or float32 values.
    Decoded(Bytes),
}

impl Held {
    /// Bytes this entry is counted as.
    fn size(&self) -> u64 {
        // The map entry and key cost something too; 64 bytes keeps a
        // cache full of headers from growing without bound.
        64 + match self {
            Self::Missing => 0,
            Self::Compressed(bytes) | Self::Decoded(bytes) => bytes.len() as u64,
        }
    }
}

type Key = (String, String, Part);

#[derive(Debug, Default)]
struct Lru {
    entries: BTreeMap<Key, (Held, u64, u64)>,
    /// Last use → key; the smallest is the next to go.
    order: BTreeMap<u64, Key>,
    total: u64,
    clock: u64,
}

impl Lru {
    fn touch(&mut self, key: &Key) {
        self.clock += 1;
        let clock = self.clock;
        if let Some((_, _, used)) = self.entries.get_mut(key) {
            self.order.remove(used);
            *used = clock;
            self.order.insert(clock, key.clone());
        }
    }
}

/// Counters for how the memory has been used.
#[derive(Debug, Default)]
pub struct MemoryStats {
    /// Reads answered from memory.
    pub hits: AtomicU64,
    /// Reads that had to download.
    pub misses: AtomicU64,
    /// Entries dropped to stay under the limit.
    pub evictions: AtomicU64,
}

/// A size-limited, least-recently-used store of downloaded blocks, in
/// memory, shared by every read of a session.
#[derive(Debug)]
pub struct BlockCache {
    limit: u64,
    lru: Mutex<Lru>,
    stats: MemoryStats,
}

impl BlockCache {
    /// Keeps the session's downloaded blocks without an application-imposed
    /// memory cap. Allocation follows the weather actually requested.
    pub fn unbounded() -> Arc<Self> {
        Self::new(u64::MAX)
    }

    /// An empty store holding at most `limit_bytes`.
    pub fn new(limit_bytes: u64) -> Arc<Self> {
        Arc::new(Self {
            limit: limit_bytes,
            lru: Mutex::new(Lru::default()),
            stats: MemoryStats::default(),
        })
    }

    /// The limit, bytes.
    pub fn limit(&self) -> u64 {
        self.limit
    }

    /// Bytes held now.
    pub fn size(&self) -> u64 {
        self.lru.lock().map_or(0, |lru| lru.total)
    }

    /// `(hits, misses, evictions)` so far.
    pub fn stats(&self) -> (u64, u64, u64) {
        (
            self.stats.hits.load(Ordering::Relaxed),
            self.stats.misses.load(Ordering::Relaxed),
            self.stats.evictions.load(Ordering::Relaxed),
        )
    }

    fn key(namespace: &str, chunk: &str, part: Part) -> Key {
        (namespace.to_owned(), chunk.to_owned(), part)
    }

    /// The entry for `part` of `chunk`, marking it used.
    pub fn get(&self, namespace: &str, chunk: &str, part: Part) -> Option<Held> {
        let key = Self::key(namespace, chunk, part);
        let mut lru = self.lru.lock().ok()?;
        let found = lru.entries.get(&key).map(|(held, ..)| held.clone());
        match found {
            Some(held) => {
                lru.touch(&key);
                self.stats.hits.fetch_add(1, Ordering::Relaxed);
                Some(held)
            }
            None => {
                self.stats.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// Whether `part` of `chunk` is held, without marking it used (the
    /// pre-flight estimate asks this of every block a fetch would need).
    pub fn contains(&self, namespace: &str, chunk: &str, part: Part) -> bool {
        let key = Self::key(namespace, chunk, part);
        self.lru
            .lock()
            .is_ok_and(|lru| lru.entries.contains_key(&key))
    }

    /// Keeps `held` for `part` of `chunk`, dropping the least recently used
    /// entries to stay within the limit. An entry larger than the whole
    /// limit is not kept.
    pub fn put(&self, namespace: &str, chunk: &str, part: Part, held: Held) {
        let size = held.size();
        if size > self.limit {
            return;
        }
        let key = Self::key(namespace, chunk, part);
        let Ok(mut lru) = self.lru.lock() else {
            return;
        };
        if let Some((_, old_size, used)) = lru.entries.remove(&key) {
            lru.order.remove(&used);
            lru.total -= old_size;
        }
        lru.clock += 1;
        let clock = lru.clock;
        lru.entries.insert(key.clone(), (held, size, clock));
        lru.order.insert(clock, key);
        lru.total += size;
        while lru.total > self.limit {
            let Some((&oldest, _)) = lru.order.iter().next() else {
                break;
            };
            if let Some(victim) = lru.order.remove(&oldest)
                && let Some((_, victim_size, _)) = lru.entries.remove(&victim)
            {
                lru.total -= victim_size;
                self.stats.evictions.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Forgets `part` of `chunk`.
    pub fn remove(&self, namespace: &str, chunk: &str, part: Part) {
        let key = Self::key(namespace, chunk, part);
        if let Ok(mut lru) = self.lru.lock()
            && let Some((_, size, used)) = lru.entries.remove(&key)
        {
            lru.order.remove(&used);
            lru.total -= size;
        }
    }

    /// Forgets everything.
    pub fn clear(&self) {
        if let Ok(mut lru) = self.lru.lock() {
            *lru = Lru::default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(n: usize) -> Held {
        Held::Compressed(Bytes::from(vec![0u8; n]))
    }

    #[test]
    fn the_least_recently_used_entry_goes_first() {
        // Each entry counts its bytes plus 64.
        let memory = BlockCache::new(3 * (100 + 64));
        memory.put("ds", "a", Part::Block(0), block(100));
        memory.put("ds", "b", Part::Block(0), block(100));
        memory.put("ds", "c", Part::Block(0), block(100));
        // Using `a` makes `b` the oldest.
        assert!(memory.get("ds", "a", Part::Block(0)).is_some());
        memory.put("ds", "d", Part::Block(0), block(100));
        assert!(!memory.contains("ds", "b", Part::Block(0)), "b went");
        for kept in ["a", "c", "d"] {
            assert!(memory.contains("ds", kept, Part::Block(0)), "{kept} kept");
        }
        assert_eq!(memory.size(), 3 * 164);
        assert_eq!(memory.stats().2, 1);
    }

    #[test]
    fn parts_of_one_chunk_are_separate_and_contains_does_not_touch() {
        let memory = BlockCache::new(1 << 20);
        memory.put("ds", "k", Part::Head, Held::Missing);
        memory.put("ds", "k", Part::Block(3), block(10));
        assert!(memory.contains("ds", "k", Part::Head));
        assert!(!memory.contains("ds", "k", Part::Block(2)));
        assert!(!memory.contains("other", "k", Part::Head));
        assert_eq!(memory.stats().0, 0, "contains is not a hit");
        // Replacing an entry does not count it twice.
        memory.put("ds", "k", Part::Block(3), block(20));
        assert_eq!(memory.size(), 64 + 64 + 20);
        memory.clear();
        assert_eq!(memory.size(), 0);
    }

    #[test]
    fn an_entry_bigger_than_the_limit_is_not_kept() {
        let memory = BlockCache::new(100);
        memory.put("ds", "huge", Part::Block(0), block(200));
        assert_eq!(memory.size(), 0);
        assert!(memory.get("ds", "huge", Part::Block(0)).is_none());
    }
}
