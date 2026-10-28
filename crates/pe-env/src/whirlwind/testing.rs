//! The fixture archive and a loopback S3 stub, for this crate's tests and,
//! with the `testing` feature, other crates' integration tests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test support; a broken fixture should stop the test"
)]

use super::layout::{Archive, Array, AxisOrder};
use super::{ArchiveLocation, ArchiveRef, Whirlwind};
use crate::Reanalysis;
use crate::grid::{Axis, Grid};
use crate::memory::BlockCache;
use crate::store::TimeAxis;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// An independent Zstandard raw-block fixture: standard magic, single segment,
// four-byte content size, and one final uncompressed block.
pub(crate) fn zstd_raw(raw: &[u8]) -> Vec<u8> {
    let mut b = vec![0x28, 0xb5, 0x2f, 0xfd, 0xa0];
    b.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    let block = ((raw.len() as u32) << 3) | 1;
    b.extend_from_slice(&block.to_le_bytes()[..3]);
    b.extend_from_slice(raw);
    b
}

/// The fixture archive's layout: 4 hours from 2000-01-01T00Z, latitudes
/// 60N to 30S every 30°, longitudes every 45°, seven parameters.
pub(super) fn archive() -> Archive {
    archive_with(Dtype::F16, &NEEDED)
}

/// What the builder writes each parameter as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dtype {
    /// Hindsight Sync layout 2.
    F16,
    /// Hindsight Sync layout 3.
    F32,
}

const NEEDED: [&str; 7] = [
    "u10",
    "v10",
    "ucur",
    "vcur",
    "wave_direction",
    "wave_height",
    "wave_period",
];

/// [`archive`] for an archive whose `param` axis lists `names`.
pub(super) fn archive_with(dtype: Dtype, names: &[&str]) -> Archive {
    let np = names.len();
    let array = Array {
        shape: [4, np, 4, 8],
        inner: [2, np, 2, 2],
        order: AxisOrder::TimeFirst,
        storage_order: AxisOrder::TimeFirst,
        fill_value: f32::NAN,
        bands: [vec![2, 2], vec![4, 4]],
        parameters: NEEDED
            .iter()
            .map(|n| names.iter().position(|m| m == n).unwrap())
            .collect(),
        bytes_per_value: if dtype == Dtype::F16 { 2 } else { 4 },
    };
    Archive {
        data: array,
        grid: Grid {
            lat: Axis::from_values("lat", &[60., 30., 0., -30.]).unwrap(),
            lon: Axis::from_values("lon", &[-180., -135., -90., -45., 0., 45., 90., 135.]).unwrap(),
        },
        time: TimeAxis {
            first: 946684800,
            step: 3600,
            len: 4,
        },
    }
}

/// Every shard of [`archive`], by object key. Values depend only on the hour
/// and the latitude row: u10 = 2 × hour + row, v10 = 4, current (0.5,
/// −0.25), wave direction 350° on even hours and 10° on odd ones, wave
/// height 2 + hour and period 8 s.
pub fn fixture() -> BTreeMap<String, Vec<u8>> {
    fixture_with(Dtype::F16, &NEEDED)
}

/// Metadata and full-size 72-hour tiles produced by HistorySyncer's layout-4
/// writer. Routing uses pre-quantized values and -999; the base uses NaN.
pub fn historysyncer_fixture(routing: bool) -> BTreeMap<String, Vec<u8>> {
    let preset = if routing {
        "routing"
    } else {
        "climatology_base"
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/whirlwind/historysyncer-v4")
        .join(preset);
    [
        "zarr.json",
        "data/zarr.json",
        "param/zarr.json",
        "param/c/0",
        "latitude/zarr.json",
        "latitude/c/0",
        "longitude/zarr.json",
        "longitude/c/0",
        "time/zarr.json",
        "time/c/0",
        "data/c/0/0/1/3",
        "data/c/1/0/1/3",
    ]
    .into_iter()
    .map(|key| (key.to_owned(), std::fs::read(root.join(key)).unwrap()))
    .collect()
}

/// [`fixture`]'s truth values in the parameters of `names` that [`fixture`]
/// has (a parameter it does not use holds a filler 1.5), as `dtype`.
pub fn fixture_with(dtype: Dtype, names: &[&str]) -> BTreeMap<String, Vec<u8>> {
    let mut objects = metadata(dtype, names);
    for t in 0..2 {
        for y in 0..2 {
            for x in 0..2 {
                let mut shard = Vec::new();
                let mut index = Vec::new();
                for inner in 0..2 {
                    let mut raw = Vec::new();
                    for step in 0..2 {
                        for name in names {
                            for row in 0..2 {
                                for _col in 0..2 {
                                    let hour = (t * 2 + step) as f32;
                                    let value = match *name {
                                        "u10" => 2. * hour + (y * 2 + row) as f32,
                                        "v10" => 4.,
                                        "ucur" => 0.5,
                                        "vcur" => -0.25,
                                        "wave_direction" => {
                                            if step == 0 {
                                                350.
                                            } else {
                                                10.
                                            }
                                        }
                                        "wave_height" => 2. + hour,
                                        "wave_period" => 8.,
                                        _ => 1.5,
                                    };
                                    match dtype {
                                        Dtype::F16 => raw.extend_from_slice(
                                            &half::f16::from_f32(value).to_bits().to_le_bytes(),
                                        ),
                                        Dtype::F32 => raw.extend_from_slice(&value.to_le_bytes()),
                                    }
                                }
                            }
                        }
                    }
                    let encoded = zstd_raw(&raw);
                    index.extend_from_slice(&(shard.len() as u64).to_le_bytes());
                    index.extend_from_slice(&(encoded.len() as u64).to_le_bytes());
                    shard.extend(encoded);
                    assert_eq!(inner, index.len() / 16 - 1);
                }
                index.extend_from_slice(&crc32c::crc32c(&index).to_le_bytes());
                shard.extend(index);
                objects.insert(format!("data/c/{t}/0/{y}/{x}"), shard);
            }
        }
    }
    objects
}

fn zstd_array(shape: usize, data_type: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "zarr_format":3,"node_type":"array","shape":[shape],"data_type":data_type,
        "chunk_key_encoding":{"name":"default","configuration":{"separator":"/"}},
        "chunk_grid":{"name":"regular","configuration":{"chunk_shape":[shape]}},
        "codecs":[{"name":"bytes","configuration":{"endian":"little"}},{"name":"zstd"}]
    })
}

/// The metadata objects of an archive written as Hindsight Sync does: its
/// root version attribute (2 for float16, 3 for float32), the `data` array
/// of [`archive_with`]'s shape, and the `param`, `latitude`, `longitude` and
/// `time` axes.
fn metadata(dtype: Dtype, names: &[&str]) -> BTreeMap<String, Vec<u8>> {
    let np = names.len();
    let json = |v: serde_json::Value| serde_json::to_vec(&v).unwrap();
    let mut objects = BTreeMap::new();
    let version = if dtype == Dtype::F16 { 2 } else { 3 };
    objects.insert(
        "zarr.json".to_owned(),
        json(serde_json::json!({"zarr_format":3,"node_type":"group",
            "attributes":{"historysyncer_layout_version":version}})),
    );
    let mut data = zstd_array(
        4,
        serde_json::json!(if dtype == Dtype::F16 {
            "float16"
        } else {
            "float32"
        }),
    );
    data["shape"] = serde_json::json!([4, np, 4, 8]);
    data["fill_value"] = serde_json::json!("NaN");
    data["dimension_names"] = serde_json::json!(["time", "param", "latitude", "longitude"]);
    data["chunk_grid"] = serde_json::json!({"name":"rectilinear","configuration":{
        "kind":"inline","chunk_shapes":[2, np, [2, 2], [4, 4]]}});
    data["codecs"] = serde_json::json!([{"name":"sharding_indexed","configuration":{
        "chunk_shape":[2, np, 2, 2],"index_location":"end",
        "codecs":[{"name":"bytes","configuration":{"endian":"little"}},{"name":"zstd","configuration":{"level":5}}],
        "index_codecs":[{"name":"bytes","configuration":{"endian":"little"}},{"name":"crc32c"}]}}]);
    objects.insert("data/zarr.json".to_owned(), json(data));
    objects.insert(
        "param/zarr.json".to_owned(),
        json(zstd_array(
            np,
            serde_json::json!({"name":"fixed_length_utf32","configuration":{"length_bytes":56}}),
        )),
    );
    let mut raw = vec![0u8; np * 56];
    for (slot, name) in names.iter().enumerate() {
        for (i, ch) in name.chars().enumerate() {
            raw[slot * 56 + i * 4..slot * 56 + i * 4 + 4]
                .copy_from_slice(&(ch as u32).to_le_bytes());
        }
    }
    objects.insert("param/c/0".to_owned(), zstd_raw(&raw));
    let floats = |v: &[f32]| zstd_raw(&v.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<_>>());
    objects.insert(
        "latitude/zarr.json".to_owned(),
        json(zstd_array(4, "float32".into())),
    );
    objects.insert("latitude/c/0".to_owned(), floats(&[60., 30., 0., -30.]));
    objects.insert(
        "longitude/zarr.json".to_owned(),
        json(zstd_array(8, "float32".into())),
    );
    objects.insert(
        "longitude/c/0".to_owned(),
        floats(&[-180., -135., -90., -45., 0., 45., 90., 135.]),
    );
    let mut time = zstd_array(4, "int64".into());
    time["attributes"] = serde_json::json!({"units":"hours since 2000-01-01T00:00:00Z"});
    objects.insert("time/zarr.json".to_owned(), json(time));
    objects.insert(
        "time/c/0".to_owned(),
        zstd_raw(&(0..4i64).flat_map(i64::to_le_bytes).collect::<Vec<_>>()),
    );
    objects
}

/// The bucket and prefix the stub serves under, path style.
pub const BUCKET: &str = "whirlwind-hindsight";
/// See [`BUCKET`].
pub const PREFIX: &str = "hindsight-routing";

/// A loopback stub answering path-style S3 reads of the given objects under
/// `/{BUCKET}/{PREFIX}/`; unless made by [`serve_signed`] it refuses any
/// signed or `x-amz-` request.
#[derive(Debug)]
pub struct Server {
    endpoint: String,
    /// Every request as `(key, Range header)`.
    pub requests: Arc<Mutex<Vec<(String, String)>>>,
    /// Every request's line and headers, as received.
    pub headers: Arc<Mutex<Vec<String>>>,
    /// The most requests answered at once.
    pub peak: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    /// Whether a chunk was requested while the slow index was still due.
    pub pipelined: Arc<AtomicBool>,
}

impl Server {
    /// The stub's base URL, `http://127.0.0.1:<port>`.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            t.join().unwrap();
        }
    }
}
/// Serves `objects`; any `status` but 200 or 206 answers every request
/// with that status.
pub fn serve(objects: BTreeMap<String, Vec<u8>>, status: u16) -> Server {
    serve_revision(objects, status, "fixture-v1")
}
/// [`serve`] with the shard ETag `revision`.
pub fn serve_revision(objects: BTreeMap<String, Vec<u8>>, status: u16, revision: &str) -> Server {
    serve_delayed(objects, status, revision, None)
}
/// [`serve_revision`], answering the index of `slow_index` late.
pub fn serve_delayed(
    objects: BTreeMap<String, Vec<u8>>,
    status: u16,
    revision: &str,
    slow_index: Option<String>,
) -> Server {
    serve_with(objects, status, revision, slow_index, false)
}
/// [`serve`], accepting signed requests (and recording them in
/// [`Server::headers`]).
pub fn serve_signed(objects: BTreeMap<String, Vec<u8>>) -> Server {
    serve_with(objects, 206, "fixture-v1", None, true)
}
fn serve_with(
    objects: BTreeMap<String, Vec<u8>>,
    status: u16,
    revision: &str,
    slow_index: Option<String>,
    signed: bool,
) -> Server {
    let revision = revision.to_owned();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let headers = Arc::new(Mutex::new(Vec::new()));
    let live = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let index_finished = Arc::new(AtomicBool::new(false));
    let pipelined = Arc::new(AtomicBool::new(false));
    let pipelined_result = Arc::clone(&pipelined);
    let (r, p, s) = (Arc::clone(&requests), Arc::clone(&peak), Arc::clone(&stop));
    let h = Arc::clone(&headers);
    let objects = Arc::new(objects);
    let thread = std::thread::spawn(move || {
        let mut workers = Vec::new();
        while !s.load(Ordering::SeqCst) {
            let Ok((mut socket, _)) = listener.accept() else {
                std::thread::sleep(Duration::from_millis(2));
                continue;
            };
            let h = Arc::clone(&h);
            let (r, p, live, objects) = (
                Arc::clone(&r),
                Arc::clone(&p),
                Arc::clone(&live),
                Arc::clone(&objects),
            );
            let revision = revision.clone();
            let (slow_index, index_finished, pipelined) = (
                slow_index.clone(),
                Arc::clone(&index_finished),
                Arc::clone(&pipelined),
            );
            workers.push(std::thread::spawn(move || {
                socket.set_nonblocking(false).unwrap();
                socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut request = Vec::new();
                let mut buf = [0u8;2048];
                while !request.windows(4).any(|b| b == b"\r\n\r\n") {
                    let n = socket.read(&mut buf).unwrap(); if n == 0 { return; } request.extend_from_slice(&buf[..n]);
                }
                let text = String::from_utf8(request).unwrap();
                let path = text.split_whitespace().nth(1).unwrap();
                // A request outside the bucket and prefix keeps its whole path, so misses.
                let key = path.strip_prefix(&format!("/{BUCKET}/{PREFIX}/")).unwrap_or(path).to_owned();
                let range = text.lines().find_map(|line| line.strip_prefix("range: ")).unwrap_or("").to_owned();
                if !signed {
                    assert!(!text.to_ascii_lowercase().contains("authorization:"));
                    assert!(!text.to_ascii_lowercase().contains("x-amz-"));
                }
                h.lock().unwrap().push(text.clone());
                r.lock().unwrap().push((key.clone(),range.clone()));
                if slow_index.as_ref() == Some(&key) && range.starts_with("bytes=-") {
                    std::thread::sleep(Duration::from_millis(250));
                    index_finished.store(true, Ordering::SeqCst);
                } else if slow_index.is_some() && !range.starts_with("bytes=-") && !index_finished.load(Ordering::SeqCst) {
                    pipelined.store(true, Ordering::SeqCst);
                }
                let now = live.fetch_add(1,Ordering::SeqCst)+1; p.fetch_max(now,Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(35));
                let reply = if status != 200 && status != 206 { format!("HTTP/1.1 {status} Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes() }
                else if let Some(body) = objects.get(&key).filter(|_| range.is_empty()) {
                    let mut response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).into_bytes();
                    response.extend_from_slice(body); response
                }
                else if let Some(body) = objects.get(&key) {
                    let bytes = range.strip_prefix("bytes=").expect("only byte ranges for field data");
                    let (a,b) = bytes.split_once('-').unwrap();
                    let (start,end) = if a.is_empty() { (body.len()-b.parse::<usize>().unwrap(),body.len()) }
                        else { (a.parse::<usize>().unwrap(),b.parse::<usize>().unwrap()+1) };
                    let mut response = format!("HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nETag: \"{revision}\"\r\nConnection: close\r\n\r\n",end-start,start,end-1,body.len()).into_bytes();
                    response.extend_from_slice(&body[start..end]); response
                } else { b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec() };
                let _ = socket.write_all(&reply);
                live.fetch_sub(1,Ordering::SeqCst);
            }));
        }
        for worker in workers {
            worker.join().unwrap();
        }
    });
    Server {
        endpoint,
        requests,
        headers,
        peak,
        stop,
        thread: Some(thread),
        pipelined: pipelined_result,
    }
}
/// An anonymous path-style location of the fixture archive on the stub at
/// `endpoint` (a [`Server`]'s).
pub fn location(endpoint: &str) -> ArchiveLocation {
    ArchiveLocation::S3 {
        bucket: BUCKET.into(),
        prefix: PREFIX.into(),
        endpoint_url: Some(endpoint.to_owned()),
        region: "us-east-1".into(),
        force_path_style: true,
        credentials: None,
    }
}

/// A reader of the fixture archive at `reference`, with `concurrency`
/// requests at once; its layout is set, so only shards are read.
pub(crate) fn whirlwind_for(
    reference: ArchiveRef,
    concurrency: usize,
    memory: Arc<BlockCache>,
) -> Whirlwind {
    whirlwind_with(reference, concurrency, memory, archive())
}

/// [`whirlwind_for`] with the layout `archive` instead of [`archive`]'s.
pub(super) fn whirlwind_with(
    reference: ArchiveRef,
    concurrency: usize,
    memory: Arc<BlockCache>,
    archive: Archive,
) -> Whirlwind {
    let w = Whirlwind::new(reference, Duration::from_secs(5), memory, concurrency).unwrap();
    *w.archive.lock().unwrap() = Some(Arc::new(archive));
    w
}

/// A reader whose layout is not preset: it opens the archive from the
/// metadata the stub at `endpoint` serves.
#[cfg(test)]
pub(super) fn whirlwind_opening(
    endpoint: &str,
    concurrency: usize,
    memory: Arc<BlockCache>,
) -> Whirlwind {
    let reference = ArchiveRef {
        identity: format!("s3://{BUCKET}/{PREFIX}"),
        location: location(endpoint),
    };
    Whirlwind::new(reference, Duration::from_secs(5), memory, concurrency).unwrap()
}

/// A reader of the fixture archive over S3 at `endpoint` (a [`Server`]'s),
/// with `concurrency` requests at once.
pub(crate) fn whirlwind_at(
    endpoint: &str,
    concurrency: usize,
    memory: Arc<BlockCache>,
) -> Whirlwind {
    let reference = ArchiveRef {
        identity: format!("s3://{BUCKET}/{PREFIX}"),
        location: location(endpoint),
    };
    whirlwind_for(reference, concurrency, memory)
}

/// The provider reading the fixture archive from the stub at `endpoint`.
pub fn reader_at(endpoint: &str) -> Reanalysis {
    let memory = BlockCache::new(16 << 20);
    Reanalysis::from_reader(whirlwind_at(endpoint, 4, Arc::clone(&memory)), memory)
}

/// Writes `objects` (a [`fixture`]) under `dir`, one file per key.
pub fn write_fixture_objects(dir: &std::path::Path, objects: &BTreeMap<String, Vec<u8>>) {
    for (key, bytes) in objects {
        let path = dir.join(key);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
}

/// Writes the [`fixture`] archive under `dir`.
pub fn write_fixture(dir: &std::path::Path) {
    write_fixture_objects(dir, &fixture());
}

/// The folder `dir` as an archive location.
pub fn file_location(dir: &std::path::Path) -> ArchiveLocation {
    ArchiveLocation::File {
        path: dir.to_owned(),
    }
}
