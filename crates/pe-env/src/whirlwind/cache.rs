//! Bounded, persistent storage of validated Whirlwind inner chunks.
//! Only this format's files in our own subdirectory are ever removed.
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock, Weak};
use std::time::SystemTime;

use bytes::Bytes;
use sha2::{Digest, Sha256};

const DIRECTORY: &str = "whirlwind-hindsight-v1";
const MAGIC: &[u8; 8] = b"PEWWC001";
const OVERHEAD: u64 = 40;
const MAX_ENTRY: u64 = (16 << 20) + OVERHEAD;

type Registry = Mutex<BTreeMap<PathBuf, Weak<DiskCache>>>;
static REGISTRY: OnceLock<Registry> = OnceLock::new();

#[derive(Debug)]
struct Entry {
    size: u64,
    used: SystemTime,
    serial: u64,
}
#[derive(Debug)]
struct State {
    entries: BTreeMap<String, Entry>,
    total: u64,
    limit: u64,
    generation: u64,
    serial: u64,
    pending: BTreeSet<String>,
    reserved: u64,
}

/// Shared across provider replacements. All filesystem work belongs on a
/// blocking worker. A clear invalidates writes from already-running reads.
#[derive(Debug)]
pub struct DiskCache {
    directory: PathBuf,
    state: Mutex<State>,
    /// Chunk I/O runs in parallel; clear and limit changes wait for publication.
    io: RwLock<()>,
}

fn lock<T>(value: &Mutex<T>) -> io::Result<std::sync::MutexGuard<'_, T>> {
    value
        .lock()
        .map_err(|_| io::Error::other("Whirlwind cache lock poisoned"))
}
fn filename(name: &str) -> bool {
    name.strip_suffix(".wwc")
        .is_some_and(|hash| hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
}

impl DiskCache {
    /// `base` is the selected directory; the cache owns just its named child.
    pub fn open(base: &Path, limit: u64) -> io::Result<Arc<Self>> {
        fs::create_dir_all(base)?;
        let directory = base.canonicalize()?.join(DIRECTORY);
        let mut registry = lock(REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new())))?;
        if let Some(cache) = registry.get(&directory).and_then(Weak::upgrade) {
            cache.set_limit(limit)?;
            return Ok(cache);
        }
        registry.retain(|_, value| value.strong_count() > 0);
        match fs::symlink_metadata(&directory) {
            Ok(meta) if !meta.file_type().is_dir() => {
                return Err(io::Error::other(
                    "Whirlwind cache path is not a regular directory",
                ));
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => fs::create_dir(&directory)?,
            Err(e) => return Err(e),
        }
        let marker = directory.join("cache-format");
        if marker.exists() {
            if fs::symlink_metadata(&marker)?.file_type().is_symlink()
                || fs::read(&marker)? != MAGIC
            {
                return Err(io::Error::other("Unrecognised Whirlwind cache directory"));
            }
        } else {
            if fs::read_dir(&directory)?.next().is_some() {
                return Err(io::Error::other(
                    "Whirlwind cache directory contains unrelated files",
                ));
            }
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&marker)?
                .write_all(MAGIC)?;
        }
        let mut entries = BTreeMap::new();
        let mut total = 0;
        for item in fs::read_dir(&directory)? {
            let item = item?;
            let name = item.file_name().to_string_lossy().into_owned();
            if !item.file_type()?.is_file() {
                continue;
            }
            // A process may have stopped while publishing a file. These
            // partial writes can never be used as chunks or escape the cap.
            if name.split_once(".wwc.").is_some_and(|(hash, rest)| {
                filename(&format!("{hash}.wwc"))
                    && rest.strip_suffix(".part").is_some_and(|ids| {
                        ids.split('.').count() == 2
                            && ids
                                .split('.')
                                .all(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
                    })
            }) {
                fs::remove_file(item.path())?;
                continue;
            }
            if !filename(&name) {
                continue;
            }
            let meta = item.metadata()?;
            entries.insert(
                name,
                Entry {
                    size: meta.len(),
                    used: meta.modified()?,
                    serial: 0,
                },
            );
            total += meta.len();
        }
        let cache = Arc::new(Self {
            directory: directory.clone(),
            state: Mutex::new(State {
                entries,
                total,
                limit,
                generation: 0,
                serial: 0,
                pending: BTreeSet::new(),
                reserved: 0,
            }),
            io: RwLock::new(()),
        });
        cache.set_limit(limit)?;
        registry.insert(directory, Arc::downgrade(&cache));
        Ok(cache)
    }

    /// Includes archive identity, shard revision and byte extent; never a path
    /// supplied by S3, a credential, or a location outside the owned directory.
    pub fn key(archive: &str, shard: &str, etag: &str, offset: u64, len: u64) -> String {
        let identity = format!("{archive}\n{shard}\n{etag}\n{offset}:{len}");
        format!("{:x}.wwc", Sha256::digest(identity.as_bytes()))
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn size(&self) -> io::Result<u64> {
        Ok(lock(&self.state)?.total)
    }
    pub fn generation(&self) -> io::Result<u64> {
        Ok(lock(&self.state)?.generation)
    }

    fn remove(&self, state: &mut State, key: &str) -> io::Result<()> {
        match fs::remove_file(self.directory.join(key)) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        if let Some(entry) = state.entries.remove(key) {
            state.total -= entry.size;
        }
        Ok(())
    }
    fn trim(&self, state: &mut State, target: u64) -> io::Result<()> {
        while state.total > target {
            let Some(oldest) = state
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.remove(state, &oldest)?;
        }
        Ok(())
    }
    pub fn set_limit(&self, limit: u64) -> io::Result<()> {
        let _io = self
            .io
            .write()
            .map_err(|_| io::Error::other("Whirlwind cache I/O lock poisoned"))?;
        let mut state = lock(&self.state)?;
        self.trim(&mut state, limit)?;
        state.limit = limit;
        Ok(())
    }
    /// A corrupt or removed entry is a cache miss, so it is downloaded again.
    pub fn get(&self, key: &str) -> io::Result<Option<Bytes>> {
        let _io = self
            .io
            .read()
            .map_err(|_| io::Error::other("Whirlwind cache I/O lock poisoned"))?;
        if !filename(key) {
            return Ok(None);
        }
        let Some(serial) = lock(&self.state)?.entries.get(key).map(|e| e.serial) else {
            return Ok(None);
        };
        let path = self.directory.join(key);
        let valid = fs::symlink_metadata(&path)
            .is_ok_and(|m| m.file_type().is_file() && m.len() <= MAX_ENTRY);
        let bytes = if valid { fs::read(&path).ok() } else { None };
        let bytes = bytes.filter(|b| {
            b.len() >= OVERHEAD as usize
                && &b[..8] == MAGIC
                && Sha256::digest(&b[40..])[..] == b[8..40]
        });
        let Some(bytes) = bytes else {
            let mut state = lock(&self.state)?;
            // Another writer may have replaced an evicted entry during I/O.
            if state.entries.get(key).is_some_and(|e| e.serial == serial) {
                self.remove(&mut state, key)?;
            }
            return Ok(None);
        };
        let now = SystemTime::now();
        // Windows needs a writable handle for file timestamps. A read-only
        // volume can still supply a valid cached chunk; touch is best effort.
        let _ = OpenOptions::new()
            .write(true)
            .open(&path)
            .and_then(|file| file.set_modified(now));
        if let Some(entry) = lock(&self.state)?.entries.get_mut(key) {
            entry.used = now;
        }
        Ok(Some(Bytes::copy_from_slice(&bytes[40..])))
    }
    /// Checks the file's size for estimates without reading all its contents.
    pub fn contains(&self, key: &str, len: u64) -> io::Result<bool> {
        let state = lock(&self.state)?;
        Ok(state.entries.contains_key(key)
            && fs::symlink_metadata(self.directory.join(key))
                .is_ok_and(|m| m.file_type().is_file() && m.len() == len + OVERHEAD))
    }
    pub fn put(&self, key: &str, bytes: &Bytes, generation: u64) -> io::Result<()> {
        let _io = self
            .io
            .read()
            .map_err(|_| io::Error::other("Whirlwind cache I/O lock poisoned"))?;
        let mut state = lock(&self.state)?;
        let size = bytes.len() as u64 + OVERHEAD;
        if generation != state.generation
            || size > state.limit
            || size > MAX_ENTRY
            || !filename(key)
        {
            return Ok(());
        }
        if state.entries.contains_key(key)
            || state.pending.contains(key)
            || state.reserved + size > state.limit
        {
            return Ok(());
        }
        // Make room before writing, including the temporary file in the cap.
        let target = state.limit - size;
        self.trim(&mut state, target)?;
        state.serial += 1;
        let serial = state.serial;
        let temporary =
            self.directory
                .join(format!("{key}.{}.{}.part", std::process::id(), serial));
        state.pending.insert(key.to_owned());
        state.total += size;
        state.reserved += size;
        drop(state);
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(MAGIC)?;
            file.write_all(&Sha256::digest(bytes))?;
            file.write_all(bytes)?;
            drop(file);
            fs::rename(&temporary, self.directory.join(key))
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        let mut state = lock(&self.state)?;
        state.pending.remove(key);
        state.reserved -= size;
        if result.is_ok() {
            state.entries.insert(
                key.to_owned(),
                Entry {
                    size,
                    used: SystemTime::now(),
                    serial,
                },
            );
        } else {
            state.total -= size;
        }
        result
    }
    /// Leaves the chosen directory and unrelated files intact.
    pub fn clear(&self) -> io::Result<()> {
        let _io = self
            .io
            .write()
            .map_err(|_| io::Error::other("Whirlwind cache I/O lock poisoned"))?;
        let mut state = lock(&self.state)?;
        state.generation += 1;
        for key in state.entries.keys().cloned().collect::<Vec<_>>() {
            self.remove(&mut state, &key)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    pub(super) struct Root(PathBuf);
    impl Root {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!(
                "pe-whirlwind-cache-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            )))
        }
    }
    impl Drop for Root {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    const IDENTITY: &str = "s3://whirlwind-hindsight/hindsight-routing";
    fn key(name: &str) -> String {
        DiskCache::key(IDENTITY, name, "v1", 0, 100)
    }
    fn bytes() -> Bytes {
        Bytes::from(vec![7; 100])
    }

    #[test]
    fn parallel_writes_reads_and_clear_stay_bounded_and_publish_whole_files() {
        let root = Root::new();
        let cache = DiskCache::open(&root.0, 32 * 140).unwrap();
        std::thread::scope(|scope| {
            for worker in 0..8 {
                let cache = &cache;
                scope.spawn(move || {
                    for i in 0..40 {
                        let key = key(&format!("{worker}-{i}"));
                        let generation = cache.generation().unwrap();
                        cache.put(&key, &bytes(), generation).unwrap();
                        if let Some(found) = cache.get(&key).unwrap() {
                            assert_eq!(found, bytes());
                        }
                        assert!(cache.size().unwrap() <= 32 * 140);
                    }
                });
            }
            let cache = &cache;
            scope.spawn(move || {
                for _ in 0..10 {
                    cache.clear().unwrap();
                }
            });
        });
        let state = cache.state.lock().unwrap();
        assert!(state.pending.is_empty());
        assert_eq!(state.reserved, 0);
        assert_eq!(
            state.total,
            state.entries.values().map(|e| e.size).sum::<u64>()
        );
        assert!(
            fs::read_dir(&cache.directory).unwrap().all(|f| !f
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".part"))
        );
    }

    #[test]
    fn survives_restart_and_reuses_the_same_directory() {
        let root = Root::new();
        let cache = DiskCache::open(&root.0, 1000).unwrap();
        assert!(Arc::ptr_eq(
            &cache,
            &DiskCache::open(&root.0, 1000).unwrap()
        ));
        cache.put(&key("a"), &bytes(), 0).unwrap();
        drop(cache);
        let restarted = DiskCache::open(&root.0, 1000).unwrap();
        assert_eq!(restarted.get(&key("a")).unwrap().unwrap(), bytes());
        assert_eq!(restarted.size().unwrap(), 140);
        assert_ne!(key("a"), DiskCache::key(IDENTITY, "a", "v2", 0, 100));
        assert_ne!(key("a"), DiskCache::key(IDENTITY, "a", "v1", 100, 100));
        assert_ne!(
            key("a"),
            DiskCache::key(
                "r2://whirlwind-hindsight/hindsight-routing",
                "a",
                "v1",
                0,
                100
            )
        );
    }
    #[test]
    fn evicts_least_recently_used_and_applies_a_smaller_limit() {
        let root = Root::new();
        let cache = DiskCache::open(&root.0, 280).unwrap();
        cache.put(&key("a"), &bytes(), 0).unwrap();
        cache.put(&key("b"), &bytes(), 0).unwrap();
        // An earlier timestamp makes the order independent of filesystem clock resolution.
        cache
            .state
            .lock()
            .unwrap()
            .entries
            .get_mut(&key("b"))
            .unwrap()
            .used = SystemTime::UNIX_EPOCH;
        cache.get(&key("a")).unwrap();
        cache.put(&key("c"), &bytes(), 0).unwrap();
        assert!(cache.contains(&key("a"), 100).unwrap());
        assert!(!cache.contains(&key("b"), 100).unwrap());
        assert!(cache.size().unwrap() <= 280);
        cache.set_limit(140).unwrap();
        assert!(cache.size().unwrap() <= 140);
        cache
            .put(&key("large"), &Bytes::from(vec![0; 200]), 0)
            .unwrap();
        assert!(!cache.contains(&key("large"), 200).unwrap());
    }
    #[test]
    fn clear_preserves_other_files_and_invalidates_in_flight_writes() {
        let root = Root::new();
        let cache = DiskCache::open(&root.0, 1000).unwrap();
        fs::write(root.0.join("keep-me"), b"project").unwrap();
        fs::write(cache.directory.join("unrelated"), b"user data").unwrap();
        cache.put(&key("a"), &bytes(), 0).unwrap();
        cache.clear().unwrap();
        cache.put(&key("a"), &bytes(), 0).unwrap();
        assert_eq!(cache.size().unwrap(), 0);
        assert_eq!(fs::read(root.0.join("keep-me")).unwrap(), b"project");
        assert_eq!(
            fs::read(cache.directory.join("unrelated")).unwrap(),
            b"user data"
        );
        cache
            .put(&key("a"), &bytes(), cache.generation().unwrap())
            .unwrap();
        assert!(cache.get(&key("a")).unwrap().is_some());
    }
    #[test]
    fn corrupt_and_incomplete_entries_are_cache_misses() {
        let root = Root::new();
        let cache = DiskCache::open(&root.0, 1000).unwrap();
        for content in [vec![0; 140], vec![7; 10]] {
            cache.put(&key("a"), &bytes(), 0).unwrap();
            fs::write(cache.directory.join(key("a")), content).unwrap();
            assert!(cache.get(&key("a")).unwrap().is_none());
            assert_eq!(cache.size().unwrap(), 0);
        }
    }
    #[test]
    fn concurrent_writes_obey_the_shared_limit() {
        let root = Root::new();
        let cache = DiskCache::open(&root.0, 700).unwrap();
        std::thread::scope(|scope| {
            for i in 0..64 {
                let cache = &cache;
                scope.spawn(move || cache.put(&key(&i.to_string()), &bytes(), 0).unwrap());
            }
        });
        assert!(cache.size().unwrap() <= 700);
        assert_eq!(
            fs::read_dir(&cache.directory)
                .unwrap()
                .filter_map(|e| {
                    let e = e.unwrap();
                    filename(&e.file_name().to_string_lossy()).then(|| e.metadata().unwrap().len())
                })
                .sum::<u64>(),
            cache.size().unwrap()
        );
    }
    #[cfg(unix)]
    #[test]
    fn symlinks_never_redirect_reads_or_clear_into_other_files() {
        let root = Root::new();
        let cache = DiskCache::open(&root.0, 1000).unwrap();
        let outside = root.0.join("unrelated");
        fs::write(&outside, b"keep").unwrap();
        cache.put(&key("a"), &bytes(), 0).unwrap();
        fs::remove_file(cache.directory.join(key("a"))).unwrap();
        std::os::unix::fs::symlink(&outside, cache.directory.join(key("a"))).unwrap();
        assert!(cache.get(&key("a")).unwrap().is_none());
        cache.clear().unwrap();
        assert_eq!(fs::read(outside).unwrap(), b"keep");
    }
}
