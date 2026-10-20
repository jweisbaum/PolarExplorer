//! The fixture archive and a loopback S3 stub, for this crate's tests and,
//! with the `testing` feature, other crates' integration tests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test support; a broken fixture should stop the test"
)]

use super::layout::{Archive, Array, Chunk};
use super::{Source, Whirlwind};
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
    let array = |np| Array {
        shape: [4, np, 4, 8],
        inner: [2, np, 2, 2],
        bands: [vec![2, 2], vec![4, 4]],
        parameters: (0..np).collect(),
    };
    Archive {
        data: array(7),
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
    let mut objects = BTreeMap::new();
    for t in 0..2 {
        for y in 0..2 {
            for x in 0..2 {
                let mut shard = Vec::new();
                let mut index = Vec::new();
                for inner in 0..2 {
                    let mut raw = Vec::new();
                    for step in 0..2 {
                        for param in 0..7 {
                            for row in 0..2 {
                                for _col in 0..2 {
                                    let hour = (t * 2 + step) as f32;
                                    let value = match param {
                                        0 => 2. * hour + (y * 2 + row) as f32,
                                        1 => 4.,
                                        2 => 0.5,
                                        3 => -0.25,
                                        4 => {
                                            if step == 0 {
                                                350.
                                            } else {
                                                10.
                                            }
                                        }
                                        5 => 2. + hour,
                                        _ => 8.,
                                    };
                                    raw.extend_from_slice(
                                        &half::f16::from_f32(value).to_bits().to_le_bytes(),
                                    );
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
                objects.insert(
                    Chunk {
                        time: t,
                        lat: y,
                        lon: x,
                        inner: 0,
                    }
                    .key(),
                    shard,
                );
            }
        }
    }
    objects
}

/// A loopback stub answering S3 reads of the given objects; it refuses
/// any signed or `x-amz-` request.
#[derive(Debug)]
pub struct Server {
    endpoint: String,
    /// Every request as `(key, Range header)`.
    pub requests: Arc<Mutex<Vec<(String, String)>>>,
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
    let revision = revision.to_owned();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let live = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let index_finished = Arc::new(AtomicBool::new(false));
    let pipelined = Arc::new(AtomicBool::new(false));
    let pipelined_result = Arc::clone(&pipelined);
    let (r, p, s) = (Arc::clone(&requests), Arc::clone(&peak), Arc::clone(&stop));
    let objects = Arc::new(objects);
    let thread = std::thread::spawn(move || {
        let mut workers = Vec::new();
        while !s.load(Ordering::SeqCst) {
            let Ok((mut socket, _)) = listener.accept() else {
                std::thread::sleep(Duration::from_millis(2));
                continue;
            };
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
                let key = text.split_whitespace().nth(1).unwrap().trim_start_matches('/').to_owned();
                let range = text.lines().find_map(|line| line.strip_prefix("range: ")).unwrap_or("").to_owned();
                assert!(!text.to_ascii_lowercase().contains("authorization:"));
                assert!(!text.to_ascii_lowercase().contains("x-amz-"));
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
        peak,
        stop,
        thread: Some(thread),
        pipelined: pipelined_result,
    }
}
/// A reader of the fixture archive over S3 at `endpoint` (a [`Server`]'s),
/// with `concurrency` requests at once.
pub(crate) fn whirlwind_at(
    endpoint: &str,
    concurrency: usize,
    memory: Arc<BlockCache>,
) -> Whirlwind {
    let mut w = Whirlwind::new(
        Source::S3,
        Source::S3.credentials(),
        Duration::from_secs(5),
        memory,
        concurrency,
    )
    .unwrap();
    w.s3.endpoint = Some(endpoint.to_owned());
    *w.archive.lock().unwrap() = Some(Arc::new(archive()));
    w
}

/// The provider reading the fixture archive from the stub at `endpoint`.
pub fn reader_at(endpoint: &str) -> Reanalysis {
    let memory = BlockCache::new(16 << 20);
    Reanalysis::from_reader(whirlwind_at(endpoint, 4, Arc::clone(&memory)), memory)
}
