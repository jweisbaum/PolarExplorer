use super::testing::{
    self, Server, archive, fixture, serve, serve_delayed, serve_revision, zstd_raw,
};
use super::*;
use crate::Dataset;

fn reader(server: &Server, concurrency: usize) -> Whirlwind {
    testing::whirlwind_at(server.endpoint(), concurrency, BlockCache::new(16 << 20))
}

#[test]
fn public_s3_reads_metadata_and_chunk_ranges_without_signing() {
    assert!(Source::S3.credentials().is_none());
    assert!(Source::R2.credentials().is_some());
    assert!(Source::Tigris.credentials().is_some());
    let mut objects = fixture();
    objects.insert("zarr.json".into(), b"{\"zarr_format\":3}".to_vec());
    let server = serve(objects, 206);
    let w = reader(&server, 4);
    let cancel = Arc::new(AtomicBool::new(false));
    let metadata = w
        .runtime
        .block_on(w.s3.get("zarr.json", None, &cancel))
        .unwrap()
        .unwrap();
    assert_eq!(&metadata[..], b"{\"zarr_format\":3}");
    let out = w.sample(&[point()], &options(), &cancel).unwrap();
    assert!(out[0].wind.is_some() && out[0].waves.is_some() && out[0].current.is_some());
    let requests = server.requests.lock().unwrap();
    assert!(requests.iter().any(|(_, range)| range.is_empty()));
    assert!(
        requests
            .iter()
            .any(|(_, range)| range.starts_with("bytes=-"))
    );
    assert!(
        requests
            .iter()
            .any(|(_, range)| range.starts_with("bytes=") && !range.starts_with("bytes=-"))
    );
}
fn options() -> Options {
    Options {
        interval: crate::Interval::Hourly,
        stokes_drift: false,
        parts: crate::Parts::ALL,
    }
}
fn point() -> Point {
    Point {
        t: 946684800 + 1800,
        lat: 45.,
        lon: -80.,
    }
}

#[test]
fn sparse_ranges_interpolate_and_reuse_cache() {
    let server = serve(fixture(), 206);
    let w = reader(&server, 4);
    let cancel = Arc::new(AtomicBool::new(false));
    let out = w.sample(&[point(), point()], &options(), &cancel).unwrap();
    assert_eq!(out[0], out[1]);
    let wind = out[0].wind.unwrap();
    assert_eq!((wind.u, wind.v), (1.5, 4.)); // half an hour + half a latitude cell
    assert_eq!(wind.dataset, Dataset::WhirlwindHindsight);
    let waves = out[0].waves.unwrap();
    assert!((waves.from.unwrap() % 360.).abs() < 1e-5);
    assert_eq!((waves.hs, waves.period_s), (Some(2.5), Some(8.)));
    assert_eq!(
        (out[0].current.unwrap().u, out[0].current.unwrap().v),
        (0.5, -0.25)
    );
    let before = server.requests.lock().unwrap().clone();
    assert_eq!(
        before.len(),
        2,
        "one index and one shared inner chunk for all seven parameters"
    );
    assert!(before.iter().all(|(k, _)| k == "data/c/0/0/0/0"));
    assert!(before.iter().all(|(_, r)| !r.is_empty()));
    assert_eq!(
        w.sample(&[point()], &options(), &cancel).unwrap()[0],
        out[0]
    );
    assert_eq!(server.requests.lock().unwrap().len(), before.len());
    assert_eq!(w.estimate(&[point()]).unwrap().hourly_bytes, 0);
}

#[test]
fn missing_shards_are_missing_but_forbidden_is_an_error() {
    for status in [404, 403] {
        let server = serve(BTreeMap::new(), status);
        let w = reader(&server, 2);
        let result = w.sample(&[point()], &options(), &Arc::new(AtomicBool::new(false)));
        if status == 404 {
            assert_eq!(result.unwrap(), vec![EnvPoint::default()]);
        } else {
            assert!(result.unwrap_err().to_string().contains("403"));
        }
    }
}

#[test]
fn cancelled_reads_make_no_requests() {
    let server = serve(fixture(), 206);
    let w = reader(&server, 4);
    assert!(matches!(
        w.sample(&[point()], &options(), &Arc::new(AtomicBool::new(true))),
        Err(EnvError::Cancelled)
    ));
    assert!(server.requests.lock().unwrap().is_empty());
}

#[test]
fn plans_cross_time_shards_spatial_shards_and_antimeridian() {
    let a = archive();
    let points = [
        Point {
            t: a.time.first + 5400,
            lat: 15.,
            lon: 179.,
        },
        Point {
            t: a.time.first - 1,
            ..point()
        },
    ];
    let (stamps, chunks) = plan(&a, &points, &options());
    assert!(stamps.iter().all(|s| s[1].is_none()));
    // Both adjacent times and both latitude/longitude bands; one array for all parameters.
    assert_eq!(chunks.len(), 8);
    assert!(chunks.keys().any(|c| c.time == 0) && chunks.keys().any(|c| c.time == 1));
    let (_, alias) = plan(
        &a,
        &[Point {
            lon: -181.,
            ..points[0]
        }],
        &options(),
    );
    assert_eq!(chunks, alias);
}

#[test]
fn corrupt_indexes_and_decoded_sizes_are_refused() {
    let chunk = Chunk {
        time: 0,
        lat: 0,
        lon: 0,
        inner: 0,
    };
    let mut index = vec![0; 16];
    index.extend_from_slice(&crc32c::crc32c(&index).to_le_bytes());
    index[0] = 1;
    assert!(
        layout::index_entry(&index, chunk)
            .unwrap_err()
            .to_string()
            .contains("checksum")
    );
    assert!(layout::decode(&zstd_raw(b"1234"), 3).is_err());
    assert!(layout::decode(&zstd_raw(b"1234"), 5).is_err());
}

#[test]
fn parallel_requests_reduce_latency_and_obey_the_limit() {
    let points = [
        point(),
        Point {
            t: 946684800 + 9000,
            lat: -15.,
            lon: 50.,
        },
    ];
    let mut elapsed = Vec::new();
    for concurrency in [1, 4] {
        let server = serve(fixture(), 206);
        let w = reader(&server, concurrency);
        let start = std::time::Instant::now();
        w.sample(&points, &options(), &Arc::new(AtomicBool::new(false)))
            .unwrap();
        elapsed.push(start.elapsed());
        assert!(server.peak.load(Ordering::SeqCst) <= concurrency);
        if concurrency == 4 {
            assert!(server.peak.load(Ordering::SeqCst) >= 2);
        }
    }
    eprintln!(
        "Whirlwind mock cold fetch: serial {:?}, parallel {:?}",
        elapsed[0], elapsed[1]
    );
}

#[test]
#[ignore = "live anonymous S3; run explicitly with PE_TEST_LIVE=1"]
fn live_whirlwind_anonymous_sample() {
    if std::env::var("PE_TEST_LIVE").as_deref() != Ok("1") {
        return;
    }
    let w = Whirlwind::new(
        Source::S3,
        Source::S3.credentials(),
        Duration::from_secs(30),
        BlockCache::new(32 << 20),
        CONCURRENCY,
    )
    .unwrap();
    let p = Point {
        t: crate::time::parse_utc("2000-01-03T12:30:00").unwrap(),
        lat: 48.,
        lon: -5.,
    };
    let start = std::time::Instant::now();
    let out = w
        .sample(&[p], &options(), &Arc::new(AtomicBool::new(false)))
        .unwrap();
    eprintln!(
        "Whirlwind live: {:?}, {:?}, {:?}",
        start.elapsed(),
        w.net_totals(),
        out
    );
    assert!(out[0].wind.is_some());
    assert!(out[0].waves.is_some());
    assert!(out[0].current.is_some());
    let counts = w.net_totals();
    assert_eq!(
        w.sample(&[p], &options(), &Arc::new(AtomicBool::new(false)))
            .unwrap(),
        out
    );
    assert_eq!(w.net_totals(), counts);
}

#[test]
fn array_metadata_validates_shapes_and_codec_before_planning() {
    let metadata = serde_json::json!({
        "zarr_format":3,"node_type":"array","shape":[4,7,4,8],
        "data_type":"float16","fill_value":"NaN","dimension_names":["time","param","latitude","longitude"],
        "chunk_key_encoding":{"name":"default","configuration":{"separator":"/"}},
        "chunk_grid":{"name":"rectilinear","configuration":{"kind":"inline","chunk_shapes":[2,7,[2,2],[4,4]]}},
        "codecs":[{"name":"sharding_indexed","configuration":{
            "chunk_shape":[2,7,2,2],"index_location":"end",
            "codecs":[{"name":"bytes","configuration":{"endian":"little"}},{"name":"zstd","configuration":{"level":5}}],
            "index_codecs":[{"name":"bytes","configuration":{"endian":"little"}},{"name":"crc32c"}]
        }}]
    });
    let a = Array::parse(&metadata, (0..7).collect()).unwrap();
    let (c, offset) = a.locate((3, 3, 7));
    assert_eq!(
        c,
        Chunk {
            time: 1,
            lat: 1,
            lon: 1,
            inner: 1
        }
    );
    assert_eq!(offset, 31); // time 1, parameter 0, row 1, column 1
    assert_eq!(a.index_len(c), 36);
    for pointer in [
        "/data_type",
        "/codecs/0/configuration/index_location",
        "/chunk_grid/configuration/chunk_shapes/2",
        "/shape/3",
    ] {
        let mut broken = metadata.clone();
        *broken.pointer_mut(pointer).unwrap() = serde_json::json!("unsupported");
        assert!(
            Array::parse(&broken, (0..7).collect()).is_err(),
            "{pointer}"
        );
    }
    assert!(Array::parse(&serde_json::json!({}), vec![]).is_err());
}

#[test]
fn disk_cache_reuses_overlapping_routes_after_restart_and_clear_refetches() {
    let path = std::env::temp_dir().join(format!("pe-whirlwind-reuse-{}", std::process::id()));
    let server = serve(fixture(), 206);
    let disk = DiskCache::open(&path, 16 << 20).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let w = reader(&server, 4).with_disk_cache(Arc::clone(&disk));
    let first = w.sample(&[point()], &options(), &cancel).unwrap();
    let cold = server.requests.lock().unwrap().len();
    assert!(disk.size().unwrap() > 0);
    drop(w);
    drop(disk);
    let disk = DiskCache::open(&path, 16 << 20).unwrap();
    let restarted = reader(&server, 4).with_disk_cache(Arc::clone(&disk));
    let estimate = restarted.estimate(&[point()]).unwrap();
    assert_eq!(estimate.hourly_bytes, 0);
    assert!(estimate.hourly_cached_bytes > 0);
    // A different route sharing the same interpolation stencil.
    let second = restarted
        .sample(
            &[
                point(),
                Point {
                    t: point().t + 60,
                    ..point()
                },
            ],
            &options(),
            &cancel,
        )
        .unwrap();
    assert_eq!(first[0], second[0]);
    assert!(
        server.requests.lock().unwrap()[cold..]
            .iter()
            .all(|(_, range)| range.starts_with("bytes=-")),
        "only fresh shard indexes are requested after restart"
    );
    let cached = server.requests.lock().unwrap().len();
    disk.clear().unwrap();
    assert!(restarted.estimate(&[point()]).unwrap().hourly_bytes > 0);
    assert_eq!(
        restarted.sample(&[point()], &options(), &cancel).unwrap(),
        first
    );
    assert!(server.requests.lock().unwrap().len() > cached);
    drop(restarted);
    drop(disk);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn a_new_shard_version_cannot_reuse_old_disk_chunks() {
    let path = std::env::temp_dir().join(format!("pe-whirlwind-version-{}", std::process::id()));
    let disk = DiskCache::open(&path, 16 << 20).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let first = serve_revision(fixture(), 206, "v1");
    let second = serve_revision(fixture(), 206, "v2");
    let w = reader(&first, 4).with_disk_cache(Arc::clone(&disk));
    w.sample(&[point()], &options(), &cancel).unwrap();
    drop(w);
    let updated = reader(&second, 4).with_disk_cache(Arc::clone(&disk));
    assert!(updated.estimate(&[point()]).unwrap().hourly_bytes > 0);
    updated.sample(&[point()], &options(), &cancel).unwrap();
    assert!(
        second
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|(_, range)| !range.starts_with("bytes=-"))
    );
    drop(updated);
    drop(disk);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn storage_sources_do_not_share_cached_chunks_or_provenance() {
    let path = std::env::temp_dir().join(format!("pe-whirlwind-sources-{}", std::process::id()));
    let disk = DiskCache::open(&path, 16 << 20).unwrap();
    let server = serve(fixture(), 206);
    let cancel = Arc::new(AtomicBool::new(false));
    for source in [Source::S3, Source::R2, Source::Tigris] {
        let mut w = reader(&server, 4).with_disk_cache(Arc::clone(&disk));
        w.s3.source = source;
        assert!(w.estimate(&[point()]).unwrap().hourly_bytes > 0);
        let out = w.sample(&[point()], &options(), &cancel).unwrap();
        assert_eq!(out[0].wind.unwrap().dataset, source.dataset());
        assert_eq!(out[0].current.unwrap().dataset, source.dataset());
        assert_eq!(out[0].waves.unwrap().dataset, source.dataset());
        assert_eq!(w.estimate(&[point()]).unwrap().hourly_bytes, 0);
    }
    // All three have the same shard key, ETag, extent and data, but each
    // source must read its own chunk before that source can reuse it.
    assert_eq!(server.requests.lock().unwrap().len(), 6);
    drop(disk);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn fetching_wind_then_waves_reuses_the_same_combined_chunk() {
    let server = serve(fixture(), 206);
    let w = reader(&server, 4);
    let cancel = Arc::new(AtomicBool::new(false));
    let mut wind = options();
    wind.parts.waves = false;
    wind.parts.current = false;
    let first = w.sample(&[point()], &wind, &cancel).unwrap();
    assert!(first[0].wind.is_some());
    assert!(first[0].waves.is_none());
    let count = server.requests.lock().unwrap().len();
    let all = w.sample(&[point()], &options(), &cancel).unwrap();
    assert!(all[0].waves.is_some());
    assert_eq!(count, 2);
    assert_eq!(server.requests.lock().unwrap().len(), count);
}

/// The S3 archive recorded 2026-10-11: 0.25° cells, so an inner chunk of
/// 20 × 20 cells is 5° × 5° (it was 40 × 40, 10° × 10°, until then). The
/// shard bands are unchanged: 20°, 70°, 60° and 30° of latitude from the
/// north; 80°, 40°, 80°, 100° and 60° of longitude from 180°W.
#[test]
fn reads_the_recorded_s3_layout_with_five_degree_inner_chunks() {
    let metadata: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../tests/fixtures/whirlwind/combined-data.zarr.json"
    ))
    .unwrap();
    let data = Array::parse(&metadata, (0..7).collect()).unwrap();
    assert_eq!(data.shape, [447072, 7, 720, 1440]);
    assert_eq!(data.inner, [72, 7, 20, 20]);
    assert_eq!(
        data.bands,
        [vec![80, 280, 240, 120], vec![320, 160, 320, 400, 240]]
    );
    // Row 203 is 123 rows into the 280-row band from 70°N; column 1107 is
    // 307 columns into the 400-column band from 20°E: a cell at 39.25°N,
    // 96.75°E.
    let (chunk, offset) = data.locate((73, 203, 1107));
    assert_eq!(chunk.key(), "data/c/1/0/1/3");
    assert_eq!(chunk.inner, 135); // inner row 6 of 14, column 15 of 20
    assert_eq!(offset, 2867); // time 1, parameter 0, row 3, column 7
    assert_eq!(data.index_len(chunk), 4484); // 14 × 20 index entries + checksum
    assert_eq!(data.decoded_len(), 403_200);
    // A position 5° away lands in another inner chunk of the same shard.
    let (east, _) = data.locate((73, 203, 1127));
    assert_eq!(east.key(), chunk.key());
    assert_eq!(east.inner, 136);
}

/// R2 and Tigris stay selectable, but since 2026-10-11 neither holds
/// weather: the request authenticates, then either the metadata is absent
/// (the reader says so) or the metadata opens and every chunk is missing
/// (missing weather, not an error). A credential or layout failure fails.
#[test]
#[ignore = "live R2 and Tigris; run explicitly with PE_TEST_LIVE=1"]
fn live_r2_and_tigris_hold_no_weather() {
    if std::env::var("PE_TEST_LIVE").as_deref() != Ok("1") {
        return;
    }
    for source in [Source::R2, Source::Tigris] {
        let w = Whirlwind::new(
            source,
            source.credentials(),
            Duration::from_secs(45),
            BlockCache::new(32 << 20),
            CONCURRENCY,
        )
        .unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let p = Point {
            t: crate::time::parse_utc("2000-01-03T12:30:00").unwrap(),
            lat: 48.,
            lon: -5.,
        };
        match w.archive(&cancel) {
            Err(error) => {
                let error = error.to_string();
                eprintln!("{source:?} live: {error} ({:?})", w.net_totals());
                assert!(
                    error.contains("Whirlwind metadata missing"),
                    "{source:?}: {error}"
                );
            }
            Ok(archive) => {
                assert_eq!(archive.data.inner, [72, 7, 20, 20]);
                let out = w.sample(&[p], &options(), &cancel).unwrap();
                eprintln!(
                    "{source:?} live: metadata only, {out:?} ({:?})",
                    w.net_totals()
                );
                assert_eq!(
                    out,
                    vec![EnvPoint::default()],
                    "{source:?} still serves weather"
                );
            }
        }
    }
}

#[test]
fn parameter_names_map_reordered_combined_bands() {
    let expected = [
        "u10",
        "v10",
        "ucur",
        "vcur",
        "wave_direction",
        "wave_height",
        "wave_period",
    ];
    let metadata = serde_json::json!({
        "zarr_format":3,"node_type":"array","shape":[7],
        "data_type":{"name":"fixed_length_utf32","configuration":{"length_bytes":56}},
        "chunk_key_encoding":{"name":"default","configuration":{"separator":"/"}},
        "chunk_grid":{"name":"regular","configuration":{"chunk_shape":[7]}},
        "codecs":[{"name":"bytes","configuration":{"endian":"little"}},{"name":"zstd"}]
    });
    let mut raw = vec![0u8; 7 * 56];
    for (slot, name) in expected.iter().rev().enumerate() {
        for (i, ch) in name.chars().enumerate() {
            raw[slot * 56 + i * 4..slot * 56 + i * 4 + 4]
                .copy_from_slice(&(ch as u32).to_le_bytes());
        }
    }
    assert_eq!(
        layout::parameters(&metadata, &zstd_raw(&raw), &expected).unwrap(),
        vec![6, 5, 4, 3, 2, 1, 0]
    );
    let mut missing = metadata;
    missing["shape"] = serde_json::json!([4]);
    assert!(layout::parameters(&missing, &zstd_raw(&raw), &expected).is_err());
}

#[test]
fn fleet_reads_each_inner_chunk_once_and_reuses_decoded_values_across_batches() {
    let server = serve(fixture(), 206);
    let w = reader(&server, 8);
    let cancel = Arc::new(AtomicBool::new(false));
    let routes: Vec<Vec<_>> = (0..50)
        .map(|boat| {
            (0..20)
                .map(|k| Point {
                    t: point().t + if k < 10 { 0 } else { 3600 },
                    lat: if boat % 2 == 0 { 45. } else { 15. },
                    lon: -160. + (boat % 4) as f64 * 45. + k as f64 * 0.01,
                })
                .collect()
        })
        .collect();
    let all: Vec<_> = routes.iter().flatten().copied().collect();
    let (_, plan) = plan(&archive(), &all, &options());
    let shards: BTreeSet<_> = plan.keys().map(|c| c.key()).collect();
    let begin = std::time::Instant::now();
    let expected = w.sample(&all, &options(), &cancel).unwrap();
    let cold = begin.elapsed();
    let requests = server.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), shards.len() + plan.len());
    assert_eq!(w.decoded_chunks.load(Ordering::Relaxed), plan.len());
    assert_eq!(
        requests.iter().collect::<BTreeSet<_>>().len(),
        requests.len(),
        "no duplicate ranges"
    );
    assert!(
        requests
            .iter()
            .all(|(_, range)| range.starts_with("bytes="))
    );
    for (p, value) in all.iter().zip(&expected) {
        let hours = (p.t - archive().time.first) as f64 / 3600.;
        assert!((value.wind.unwrap().u - (2. * hours + (60. - p.lat) / 30.)).abs() < 1e-6);
    }
    let begin = std::time::Instant::now();
    for (route, expected) in routes.iter().zip(expected.chunks(20)) {
        assert_eq!(w.sample(route, &options(), &cancel).unwrap(), expected);
    }
    let warm = begin.elapsed();
    assert_eq!(
        w.decoded_chunks.load(Ordering::Relaxed),
        plan.len(),
        "warm routes must not decompress again"
    );
    assert_eq!(server.requests.lock().unwrap().len(), requests.len());
    assert!(w.memory.size() <= w.memory.limit());
    eprintln!(
        "50 fixture routes: cold fleet {cold:?}, warm individual routes {warm:?}, {} shard indexes + {} inner chunks",
        shards.len(),
        plan.len()
    );
}

#[test]
fn chunk_downloads_start_before_unrelated_shard_indexes_finish() {
    let server = serve_delayed(fixture(), 206, "v1", Some("data/c/0/0/0/0".into()));
    let w = reader(&server, 8);
    w.sample(
        &[
            point(),
            Point {
                lon: 50.,
                ..point()
            },
        ],
        &options(),
        &Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    assert!(
        server.pipelined.load(Ordering::SeqCst),
        "a slow index must not hold up chunks from other shards"
    );
}
