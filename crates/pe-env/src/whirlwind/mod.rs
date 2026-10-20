//! Whirlwind Hindsight: sparse v3 shard reads, with async I/O and parallel decoding.
//! Indexes stay in memory; validated compressed chunks can also be kept on disk.
mod cache;
mod layout;
mod location;
mod s3;

pub use cache::DiskCache;
pub use location::{ArchiveLocation, ArchiveRef, Credentials};

use bytes::Bytes;
use futures_util::{StreamExt, TryStreamExt, stream};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::dataset::Cell;
use crate::memory::{BlockCache, Held, Part};
use crate::sampler::{Stamp, bracket_every, direction_of, scalar_of};
use crate::store::TimeAxis;
use crate::{EnvError, EnvPoint, Estimate, Options, Point, Provider, Result, Vector, Waves};
use layout::{Archive, Array, Chunk, bad};
use s3::{Range, S3};

/// S3 supports many small parallel reads; this cap bounds sockets and
/// decoding memory.
pub const CONCURRENCY: usize = 64;

type ChunkPlan = BTreeMap<Chunk, BTreeSet<Cell>>;
type Values = BTreeMap<Cell, [f32; 7]>;

#[derive(Debug)]
struct Index {
    bytes: Option<Bytes>,
    etag: Option<String>,
}

fn cache_error(error: std::io::Error) -> EnvError {
    EnvError::Cache(error.to_string())
}

#[derive(Debug)]
pub struct Whirlwind {
    reference: ArchiveRef,
    s3: S3,
    runtime: tokio::runtime::Runtime,
    memory: Arc<BlockCache>,
    concurrency: usize,
    archive: Mutex<Option<Arc<Archive>>>,
    disk: Option<Arc<DiskCache>>,
    versions: Mutex<BTreeMap<String, String>>,
    decoders: Arc<tokio::sync::Semaphore>,
    #[cfg(test)]
    decoded_chunks: std::sync::atomic::AtomicUsize,
}

impl Whirlwind {
    /// A reader of `archive`. Only S3 locations are read so far.
    ///
    /// # Errors
    /// A folder location, a bad endpoint, or the workers not starting.
    pub fn new(
        archive: ArchiveRef,
        timeout: Duration,
        memory: Arc<BlockCache>,
        concurrency: usize,
    ) -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .max_blocking_threads(concurrency.max(1))
            .enable_all()
            .build()
            .map_err(|e| EnvError::Open(format!("Whirlwind workers: {e}")))?;
        Ok(Self {
            s3: S3::new(&archive.location, timeout, concurrency)?,
            reference: archive,
            runtime,
            memory,
            concurrency: concurrency.max(1),
            archive: Mutex::new(None),
            disk: None,
            versions: Mutex::new(BTreeMap::new()),
            decoders: Arc::new(tokio::sync::Semaphore::new(
                std::thread::available_parallelism()
                    .map_or(2, usize::from)
                    .min(8),
            )),
            #[cfg(test)]
            decoded_chunks: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    /// The archive read.
    pub fn reference(&self) -> &ArchiveRef {
        &self.reference
    }

    pub fn with_disk_cache(mut self, cache: Arc<DiskCache>) -> Self {
        self.disk = Some(cache);
        self
    }

    fn generation(&self) -> Result<u64> {
        self.disk
            .as_ref()
            .map_or(Ok(0), |cache| cache.generation().map_err(cache_error))
    }

    fn chunk_namespace(&self, generation: u64) -> String {
        format!("{}/chunks/{generation}", self.reference.identity)
    }

    pub fn net_totals(&self) -> (u64, u64) {
        let (requests, bytes, _) = self.s3.stats.snapshot();
        (requests, bytes)
    }

    fn archive(&self, cancel: &AtomicBool) -> Result<Arc<Archive>> {
        let mut slot = self
            .archive
            .lock()
            .map_err(|_| bad("Whirlwind metadata lock poisoned"))?;
        if let Some(archive) = slot.as_ref() {
            return Ok(Arc::clone(archive));
        }
        let archive = Arc::new(self.runtime.block_on(self.open(cancel))?);
        *slot = Some(Arc::clone(&archive));
        Ok(archive)
    }

    async fn required(&self, key: &str, cancel: &AtomicBool) -> Result<Bytes> {
        self.s3
            .get(key, None, cancel)
            .await?
            .ok_or_else(|| bad(format!("Whirlwind metadata missing: {key}")))
    }

    async fn open(&self, cancel: &AtomicBool) -> Result<Archive> {
        let names = [
            "data/zarr.json",
            "param/zarr.json",
            "param/c/0",
            "latitude/zarr.json",
            "latitude/c/0",
            "longitude/zarr.json",
            "longitude/c/0",
            "time/zarr.json",
            "time/c/0",
        ];
        let objects: BTreeMap<_, _> = stream::iter(names)
            .map(
                |name| async move { Ok::<_, EnvError>((name, self.required(name, cancel).await?)) },
            )
            .buffer_unordered(self.concurrency)
            .try_collect()
            .await?;
        let json = |name| -> Result<Value> {
            serde_json::from_slice(&objects[name])
                .map_err(|_| bad(format!("invalid Whirlwind metadata: {name}")))
        };
        let data = Array::parse(
            &json("data/zarr.json")?,
            layout::parameters(
                &json("param/zarr.json")?,
                &objects["param/c/0"],
                &[
                    "u10",
                    "v10",
                    "ucur",
                    "vcur",
                    "wave_direction",
                    "wave_height",
                    "wave_period",
                ],
            )?,
        )?;
        let lat = layout::coordinate(
            &json("latitude/zarr.json")?,
            &objects["latitude/c/0"],
            data.shape[2],
            "float32",
            4,
        )?;
        let lon = layout::coordinate(
            &json("longitude/zarr.json")?,
            &objects["longitude/c/0"],
            data.shape[3],
            "float32",
            4,
        )?;
        let meta = json("time/zarr.json")?;
        let units = crate::time::TimeUnits::parse(
            meta["attributes"]["units"]
                .as_str()
                .ok_or_else(|| bad("time units missing"))?,
        )?;
        let raw = layout::coordinate(&meta, &objects["time/c/0"], data.shape[0], "int64", 8)?;
        let times = raw
            .chunks_exact(8)
            .map(|b| {
                i64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
                    .checked_mul(units.seconds)
                    .and_then(|v| v.checked_add(units.epoch))
                    .ok_or_else(|| bad("time overflow"))
            })
            .collect::<Result<Vec<_>>>()?;
        let first = times[0];
        if times.len() < 2
            || times
                .windows(2)
                .any(|t| t[1].checked_sub(t[0]) != Some(3600))
        {
            return Err(bad("Whirlwind requires a regular hourly time axis"));
        }
        Ok(Archive {
            grid: layout::grid(&lat, &lon)?,
            time: TimeAxis {
                first,
                step: 3600,
                len: times.len() as u64,
            },
            data,
        })
    }

    async fn index(&self, key: &str, len: usize, cancel: &AtomicBool) -> Result<Index> {
        let cached = self.memory.get(&self.reference.identity, key, Part::Head);
        let bytes = match cached {
            Some(Held::Compressed(b)) => Some(b),
            Some(Held::Missing) => None,
            _ => {
                let object = self
                    .s3
                    .get_versioned(key, Some(Range::Suffix(len as u64)), cancel)
                    .await?;
                let bytes = object
                    .map(|object| {
                        if let Some(etag) = object.etag {
                            self.versions
                                .lock()
                                .map_err(|_| bad("Whirlwind version lock poisoned"))?
                                .insert(key.to_owned(), etag);
                        }
                        Ok::<_, EnvError>(object.bytes)
                    })
                    .transpose()?;
                if let Some(bytes) = &bytes {
                    layout::index_entry(
                        bytes,
                        Chunk {
                            time: 0,
                            lat: 0,
                            lon: 0,
                            inner: 0,
                        },
                    )?;
                }
                self.memory.put(
                    &self.reference.identity,
                    key,
                    Part::Head,
                    bytes.clone().map_or(Held::Missing, Held::Compressed),
                );
                bytes
            }
        };
        let etag = self
            .versions
            .lock()
            .map_err(|_| bad("Whirlwind version lock poisoned"))?
            .get(key)
            .cloned();
        Ok(Index { bytes, etag })
    }

    async fn indexes(
        &self,
        archive: &Archive,
        plan: &ChunkPlan,
        cancel: &AtomicBool,
    ) -> Result<BTreeMap<String, Index>> {
        let shards: BTreeMap<_, _> = plan
            .keys()
            .map(|c| (c.key(), archive.data.index_len(*c)))
            .collect();
        stream::iter(shards)
            .map(|(key, len)| async move {
                let index = self.index(&key, len, cancel).await?;
                Ok::<_, EnvError>((key, index))
            })
            .buffer_unordered(self.concurrency)
            .try_collect()
            .await
    }

    async fn read(
        &self,
        archive: &Archive,
        plan: ChunkPlan,
        cancel: &AtomicBool,
    ) -> Result<Values> {
        let generation = self.generation()?;
        // Share each index request, but release its chunks as soon as it arrives.
        // A slow shard must not hold every other shard behind a global barrier.
        let indexes: BTreeMap<String, tokio::sync::OnceCell<Index>> = plan
            .keys()
            .map(|chunk| (chunk.key(), tokio::sync::OnceCell::new()))
            .collect();
        let namespace = self.chunk_namespace(generation);
        let chunks = stream::iter(plan).map(|(chunk,cells)| {
            let indexes = &indexes;
            let namespace = &namespace;
            async move {
                if cancel.load(Ordering::SeqCst) { return Err(EnvError::Cancelled); }
                let key = chunk.key();
                let array = &archive.data;
                let index = indexes[&key].get_or_try_init(|| self.index(&key, array.index_len(chunk), cancel)).await?;
                let memory_key = format!("{key}@{}",index.etag.as_deref().unwrap_or(""));
                let extent = index.bytes.as_ref().map(|bytes| layout::index_entry(bytes,chunk)).transpose()?.flatten();
                let Some((offset,len)) = extent else {
                    return Ok(cells.into_iter().map(|cell| (cell,[f32::NAN;7])).collect::<Vec<_>>());
                };
                // The shard ETag prevents reuse after archive rewrites, even if
                // an inner chunk's offset and compressed length stay identical.
                let disk_key = index.etag.as_deref().map(|etag| DiskCache::key(&self.reference.identity,&key,etag,offset,len));
                let expected = array.decoded_len();
                let raw = match self.memory.get(namespace, &memory_key, Part::Decoded(chunk.inner)) {
                    Some(Held::Decoded(raw)) => raw,
                    _ => {
                        let bytes = match self.memory.get(namespace,&memory_key,Part::Block(chunk.inner)) {
                            Some(Held::Compressed(bytes)) => bytes,
                            _ => {
                                let held = if let (Some(cache), Some(disk_key)) = (&self.disk, &disk_key) {
                                    let (cache, disk_key) = (Arc::clone(cache), disk_key.clone());
                                    tokio::task::spawn_blocking(move || cache.get(&disk_key)).await
                                        .map_err(|_| bad("Whirlwind cache worker failed"))?.map_err(cache_error)?
                                } else { None };
                                if let Some(bytes) = held { bytes } else {
                                    let object = self.s3.get_versioned(&key,Some(Range::Bytes(offset,len)),cancel).await?
                                        .ok_or_else(|| bad("Whirlwind shard disappeared after its index was read; retry the fetch"))?;
                                    if index.etag != object.etag {
                                        self.memory.remove(&self.reference.identity,&key,Part::Head);
                                        return Err(bad("Whirlwind shard changed while reading; retry the fetch"));
                                    }
                                    object.bytes
                                }
                            }
                        };
                        let permit = Arc::clone(&self.decoders).acquire_owned().await.map_err(|_| EnvError::Cancelled)?;
                        if cancel.load(Ordering::SeqCst) { return Err(EnvError::Cancelled); }
                        let encoded = bytes.clone();
                        let raw = tokio::task::spawn_blocking(move || {
                            // Keep the slot until CPU work actually stops, even
                            // if the awaiting read is cancelled or fails.
                            let _permit = permit;
                            layout::decode(&encoded, expected)
                        })
                            .await.map_err(|_| bad("Whirlwind decoder worker failed"))??;
                        #[cfg(test)]
                        self.decoded_chunks.fetch_add(1, Ordering::Relaxed);
                        let raw = Bytes::from(raw);
                        self.memory.put(namespace, &memory_key, Part::Block(chunk.inner), Held::Compressed(bytes.clone()));
                        self.memory.put(namespace, &memory_key, Part::Decoded(chunk.inner), Held::Decoded(raw.clone()));
                        if let (Some(cache), Some(key)) = (&self.disk, disk_key) {
                            let cache = Arc::clone(cache);
                            tokio::task::spawn_blocking(move || cache.put(&key, &bytes, generation)).await
                                .map_err(|_| bad("Whirlwind cache worker failed"))?.map_err(cache_error)?;
                        }
                        raw
                    }
                };
                let stride = array.inner[2]*array.inner[3];
                let width = array.bytes_per_value;
                let decoded = cells.into_iter().map(|cell| {
                    let offset = array.locate(cell).1;
                    let mut values = [f32::NAN;7];
                    for (i,param) in array.parameters.iter().enumerate() {
                        let at = (offset + param * stride)*width;
                        values[i] = if width == 2 {
                            half::f16::from_bits(u16::from_le_bytes([raw[at],raw[at+1]])).to_f32()
                        } else {
                            f32::from_le_bytes([raw[at],raw[at+1],raw[at+2],raw[at+3]])
                        };
                    }
                    (cell, values)
                }).collect::<Vec<_>>();
                Ok(decoded)
            }
        }).buffer_unordered(self.concurrency).try_collect::<Vec<_>>().await?;
        Ok(chunks.into_iter().flatten().collect())
    }

    /// Exact unique chunk accounting using the archive indexes; no field data.
    pub fn estimate(&self, points: &[Point]) -> Result<Estimate> {
        let cancel = AtomicBool::new(false);
        let archive = self.archive(&cancel)?;
        let mut result = Estimate::default();
        let namespace = self.chunk_namespace(self.generation()?);
        let options = Options {
            interval: crate::Interval::Hourly,
            stokes_drift: false,
            parts: crate::Parts::ALL,
        };
        let (_, plan) = plan(&archive, points, &options);
        let indexes = self
            .runtime
            .block_on(self.indexes(&archive, &plan, &cancel))?;
        let mut missing = 0;
        let mut cached = 0;
        for chunk in plan.keys() {
            let key = chunk.key();
            let index = &indexes[&key];
            let memory_key = format!("{key}@{}", index.etag.as_deref().unwrap_or(""));
            if let Some(bytes) = &index.bytes
                && let Some((offset, len)) = layout::index_entry(bytes, *chunk)?
            {
                let on_disk = match (&self.disk, &index.etag) {
                    (Some(cache), Some(etag)) => cache
                        .contains(
                            &DiskCache::key(&self.reference.identity, &key, etag, offset, len),
                            len,
                        )
                        .map_err(cache_error)?,
                    _ => false,
                };
                if self
                    .memory
                    .contains(&namespace, &memory_key, Part::Block(chunk.inner))
                    || self
                        .memory
                        .contains(&namespace, &memory_key, Part::Decoded(chunk.inner))
                    || on_disk
                {
                    cached += len;
                } else {
                    missing += len;
                }
            }
        }
        result.hourly_bytes = missing;
        result.hourly_cached_bytes = cached;
        Ok(result)
    }
}

fn plan(
    archive: &Archive,
    points: &[Point],
    options: &Options,
) -> (Vec<Vec<Option<Stamp>>>, ChunkPlan) {
    let enabled = [
        options.parts.wind,
        options.parts.wind,
        options.parts.current,
        options.parts.current,
        options.parts.waves,
        options.parts.waves,
        options.parts.waves,
    ];
    let mut chunks = ChunkPlan::new();
    // All parameters are hourly and occupy the same chunk. Build the stencil
    // and insert its cells once, then share it across enabled parameters.
    let wanted = enabled.iter().any(|enabled| *enabled);
    let common: Vec<Option<Stamp>> = points
        .iter()
        .map(|p| {
            if !wanted {
                return None;
            }
            let stencil = archive.grid.stencil(p.lat, p.lon)?;
            let (a, b, w) = bracket_every(&archive.time, p.t, options.interval.seconds())?;
            let rows = if stencil.lat_frac == 0.0 {
                &stencil.lat[..1]
            } else {
                &stencil.lat[..]
            };
            let cols = if stencil.lon_frac == 0.0 {
                &stencil.lon[..1]
            } else {
                &stencil.lon[..]
            };
            for step in [a, b] {
                for cell in rows
                    .iter()
                    .flat_map(|&y| cols.iter().map(move |&x| (step, y, x)))
                {
                    chunks
                        .entry(archive.data.locate(cell).0)
                        .or_default()
                        .insert(cell);
                }
            }
            Some(Stamp { stencil, a, b, w })
        })
        .collect();
    let stamps = enabled
        .into_iter()
        .map(|enabled| {
            if enabled {
                common.clone()
            } else {
                vec![None; points.len()]
            }
        })
        .collect();
    (stamps, chunks)
}

impl Provider for Whirlwind {
    fn sample(
        &self,
        points: &[Point],
        options: &Options,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Vec<EnvPoint>> {
        if cancel.load(Ordering::SeqCst) {
            return Err(EnvError::Cancelled);
        }
        if points.is_empty() {
            return Ok(Vec::new());
        }
        let archive = self.archive(cancel)?;
        let (stamps, plan) = plan(&archive, points, options);
        let values = self.runtime.block_on(self.read(&archive, plan, cancel))?;
        let sampled = stamps
            .into_iter()
            .enumerate()
            .map(|(param, stamps)| {
                let cells = values.iter().map(|(cell, v)| (*cell, v[param])).collect();
                if param == 4 {
                    direction_of(&(stamps, cells))
                } else {
                    scalar_of(&(stamps, cells))
                }
            })
            .collect::<Vec<_>>();
        let dataset = crate::Dataset::Hindsight;
        Ok((0..points.len())
            .map(|i| {
                let vector = |a: usize, b: usize| {
                    Some(Vector {
                        u: sampled[a][i]?,
                        v: sampled[b][i]?,
                        dataset,
                    })
                };
                let waves = Waves {
                    from: sampled[4][i],
                    hs: sampled[5][i],
                    period_s: sampled[6][i],
                    dataset,
                };
                EnvPoint {
                    wind: vector(0, 1),
                    current: vector(2, 3),
                    waves: (waves.from.is_some() || waves.hs.is_some() || waves.period_s.is_some())
                        .then_some(waves),
                }
            })
            .collect())
    }

    fn prepare(
        &self,
        points: &[Point],
        _options: &Options,
        cancel: &Arc<AtomicBool>,
    ) -> Result<()> {
        if !points.is_empty() {
            self.archive(cancel)?;
        }
        Ok(())
    }
}

#[cfg(any(test, feature = "testing"))]
pub mod testing;
#[cfg(test)]
mod tests;
