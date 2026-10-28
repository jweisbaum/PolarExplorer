use super::testing::{
    self, Dtype, Server, archive, fixture, fixture_with, serve, serve_delayed, serve_revision,
    zstd_raw,
};
use super::*;
use crate::Dataset;

fn reader(server: &Server, concurrency: usize) -> Whirlwind {
    testing::whirlwind_at(server.endpoint(), concurrency, BlockCache::new(16 << 20))
}

#[test]
fn public_s3_reads_metadata_and_chunk_ranges_without_signing() {
    let mut objects = fixture();
    objects.insert("zarr.json".into(), b"{\"zarr_format\":3}".to_vec());
    let server = serve(objects, 206);
    let w = reader(&server, 4);
    let cancel = Arc::new(AtomicBool::new(false));
    let metadata = w
        .runtime
        .block_on(w.transport.get("zarr.json", None, &cancel))
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
    assert_eq!(wind.dataset, Dataset::Hindsight);
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
    for status in [404, 403, 401, 400] {
        let server = serve(BTreeMap::new(), status);
        let w = reader(&server, 2);
        let result = w.sample(&[point()], &options(), &Arc::new(AtomicBool::new(false)));
        match status {
            404 => assert_eq!(result.unwrap(), vec![EnvPoint::default()]),
            // A refusal for the credentials is its own kind of error.
            401 | 403 => {
                let error = result.unwrap_err();
                assert!(
                    matches!(&error, EnvError::Auth { status: s, .. } if *s == status),
                    "{error:?}"
                );
                assert!(
                    error
                        .to_string()
                        .starts_with("the archive refused the read of ")
                );
            }
            // A 400 that names no credentials code is not one.
            _ => {
                let error = result.unwrap_err();
                assert!(matches!(&error, EnvError::Open(_)), "{error:?}");
                assert!(
                    error.to_string().contains("the archive answered 400"),
                    "{error}"
                );
            }
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
        ArchiveRef {
            identity: "s3://whirlwind-hindsight/hindsight-routing".into(),
            location: s3_bare("whirlwind-hindsight", "hindsight-routing"),
        },
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
    assert_eq!(data.key(chunk), "data/c/1/0/1/3");
    assert_eq!(chunk.inner, 135); // inner row 6 of 14, column 15 of 20
    assert_eq!(offset, 2867); // time 1, parameter 0, row 3, column 7
    assert_eq!(data.index_len(chunk), 4484); // 14 × 20 index entries + checksum
    assert_eq!(data.decoded_len(), 403_200);
    // A position 5° away lands in another inner chunk of the same shard.
    let (east, _) = data.locate((73, 203, 1127));
    assert_eq!(data.key(east), data.key(chunk));
    assert_eq!(east.inner, 136);
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
    let fixture_archive = archive();
    let (_, plan) = plan(&fixture_archive, &all, &options());
    let shards: BTreeSet<_> = plan.keys().map(|c| fixture_archive.data.key(*c)).collect();
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

fn s3(endpoint_url: Option<&str>, region: &str, path_style: bool) -> ArchiveLocation {
    ArchiveLocation::S3 {
        bucket: "whirlwind-hindsight".into(),
        prefix: "hindsight-routing".into(),
        endpoint_url: endpoint_url.map(str::to_owned),
        region: region.into(),
        force_path_style: path_style,
        credentials: None,
    }
}

#[test]
fn base_url_follows_the_provider_rules() {
    // What the template expands to, built here so the source names no bucket host.
    let (bucket, region, prefix) = ("whirlwind-hindsight", "us-east-1", "hindsight-routing");
    assert_eq!(
        s3(None, "us-east-1", false).base_url().unwrap(),
        format!("https://{bucket}.s3.{region}.amazonaws.com/{prefix}")
    );
    assert_eq!(s3(None, "us-east-1", false).place(), "S3");
    // `.invalid` (RFC 2606) stands for an R2 account host: never resolved.
    let r2 = s3(
        Some("https://acct.r2.cloudflarestorage.invalid"),
        "auto",
        true,
    );
    assert_eq!(
        r2.base_url().unwrap(),
        "https://acct.r2.cloudflarestorage.invalid/whirlwind-hindsight/hindsight-routing"
    );
    assert_eq!(r2.place(), "R2");
    assert_eq!(
        s3(Some("https://store.example.invalid"), "auto", true).place(),
        "S3"
    );
    // Trailing slashes on the endpoint and the prefix are dropped.
    let mut slashed = s3(
        Some("https://acct.r2.cloudflarestorage.invalid/"),
        "auto",
        true,
    );
    if let ArchiveLocation::S3 { prefix, .. } = &mut slashed {
        prefix.push('/');
    }
    assert_eq!(slashed.base_url().unwrap(), r2.base_url().unwrap());
    // Without path style the bucket is a subdomain of the endpoint.
    assert_eq!(
        s3(Some("https://store.example.invalid:9000"), "auto", false)
            .base_url()
            .unwrap(),
        "https://whirlwind-hindsight.store.example.invalid:9000/hindsight-routing"
    );
    let folder = ArchiveLocation::File {
        path: "/data/hindsight".into(),
    };
    assert!(matches!(folder.base_url(), Err(EnvError::Open(_))));
    assert_eq!(folder.place(), "hindsight");
    assert!(
        s3(Some("ftp://host.invalid"), "auto", true)
            .base_url()
            .is_err()
    );
}

#[test]
fn signed_reads_carry_the_location_region_and_never_the_secret() {
    let server = testing::serve_signed(fixture());
    let mut location = testing::location(server.endpoint());
    if let ArchiveLocation::S3 {
        region,
        credentials,
        ..
    } = &mut location
    {
        *region = "auto".into();
        *credentials = Some(Credentials::new("KEYID", "secret-never-sent"));
    }
    let reference = ArchiveRef {
        identity: "r2://whirlwind-hindsight/hindsight-routing".into(),
        location,
    };
    let debug = format!("{reference:?}");
    assert!(debug.contains("Credentials([redacted])") && !debug.contains("secret-never-sent"));
    let w = testing::whirlwind_for(reference, 4, BlockCache::new(16 << 20));
    let out = w
        .sample(&[point()], &options(), &Arc::new(AtomicBool::new(false)))
        .unwrap();
    assert!(out[0].wind.is_some());
    let headers = server.headers.lock().unwrap();
    assert!(!headers.is_empty());
    for request in headers.iter() {
        let auth = request
            .lines()
            .find_map(|l| l.strip_prefix("authorization: "))
            .expect("every read is signed");
        assert!(auth.starts_with("AWS4-HMAC-SHA256 Credential=KEYID/"));
        assert!(auth.contains("/auto/s3/aws4_request"), "{auth}");
        assert!(!request.contains("secret-never-sent"));
    }
}

#[test]
fn two_identities_never_share_cached_chunks() {
    let path = std::env::temp_dir().join(format!("pe-whirlwind-identities-{}", std::process::id()));
    let disk = DiskCache::open(&path, 16 << 20).unwrap();
    let memory = BlockCache::new(16 << 20);
    let server = serve(fixture(), 206);
    let cancel = Arc::new(AtomicBool::new(false));
    let mut counts = Vec::new();
    for identity in [
        "s3://whirlwind-hindsight/hindsight-routing",
        "r2://whirlwind-hindsight/hindsight-routing",
    ] {
        let reference = ArchiveRef {
            identity: identity.into(),
            location: testing::location(server.endpoint()),
        };
        let w = testing::whirlwind_for(reference, 4, Arc::clone(&memory))
            .with_disk_cache(Arc::clone(&disk));
        assert!(w.estimate(&[point()]).unwrap().hourly_bytes > 0);
        let out = w.sample(&[point()], &options(), &cancel).unwrap();
        assert_eq!(out[0].wind.unwrap().dataset, Dataset::Hindsight);
        assert_eq!(w.estimate(&[point()]).unwrap().hourly_bytes, 0);
        counts.push(server.requests.lock().unwrap().len());
    }
    // Same shard key, ETag, extent, bytes, memory and disk cache: the second
    // identity still reads its own index and chunk.
    assert_eq!(counts, [2, 4]);
    drop(disk);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn the_location_json_round_trips_hindsight_syncs_shape() {
    // Hindsight Sync's `location` objects (src/api/catalog.rs), with the
    // fields the reader ignores.
    let cloud = serde_json::json!({
        "protocol": "s3", "provider": "r2",
        "bucket": "whirlwind-hindsight", "prefix": "hindsight-routing",
        "s3_uri": "r2://whirlwind-hindsight/hindsight-routing",
        "endpoint_url": "https://acct.r2.cloudflarestorage.invalid",
        "region": "auto", "force_path_style": true,
        "credentials": {"access_key_id": "KEYID", "secret_access_key": "SECRET", "session_token": null},
        "credential_error": null,
    });
    let parsed: ArchiveLocation = serde_json::from_value(cloud.clone()).unwrap();
    assert_eq!(
        parsed,
        ArchiveLocation::S3 {
            bucket: "whirlwind-hindsight".into(),
            prefix: "hindsight-routing".into(),
            endpoint_url: Some("https://acct.r2.cloudflarestorage.invalid".into()),
            region: "auto".into(),
            force_path_style: true,
            credentials: Some(Credentials::new("KEYID", "SECRET")),
        }
    );
    let written = serde_json::to_value(&parsed).unwrap();
    let mut expected = cloud;
    for ignored in ["provider", "s3_uri", "credential_error"] {
        expected.as_object_mut().unwrap().remove(ignored);
    }
    assert_eq!(written, expected);
    assert_eq!(
        serde_json::from_value::<ArchiveLocation>(written).unwrap(),
        parsed
    );
    // Amazon S3 without credentials: no endpoint, a credential error instead.
    let aws: ArchiveLocation = serde_json::from_value(serde_json::json!({
        "protocol": "s3", "provider": "s3", "bucket": "b", "prefix": "p",
        "endpoint_url": null, "region": "us-east-1", "force_path_style": false,
        "credentials": null, "credential_error": "missing",
    }))
    .unwrap();
    assert_eq!(aws, s3_bare("b", "p"));

    let local = serde_json::json!({"protocol": "file", "path": "/Users/sailor/Hindsight/routing"});
    let parsed: ArchiveLocation = serde_json::from_value(local.clone()).unwrap();
    assert_eq!(
        parsed,
        ArchiveLocation::File {
            path: "/Users/sailor/Hindsight/routing".into()
        }
    );
    assert_eq!(serde_json::to_value(&parsed).unwrap(), local);
}

fn s3_bare(bucket: &str, prefix: &str) -> ArchiveLocation {
    ArchiveLocation::S3 {
        bucket: bucket.into(),
        prefix: prefix.into(),
        endpoint_url: None,
        region: "us-east-1".into(),
        force_path_style: false,
        credentials: None,
    }
}

const ELEVEN: [&str; 11] = [
    "mslp",
    "u10",
    "z500",
    "v10",
    "ucur",
    "vcur",
    "wave_direction",
    "wave_height",
    "wave_period",
    "wind_speed",
    "wind_constancy",
];

fn sample_opened(objects: BTreeMap<String, Vec<u8>>) -> (Vec<EnvPoint>, Arc<layout::Archive>) {
    let server = serve(objects, 206);
    let w = testing::whirlwind_opening(server.endpoint(), 4, BlockCache::new(16 << 20));
    let cancel = Arc::new(AtomicBool::new(false));
    let out = w.sample(&[point()], &options(), &cancel).unwrap();
    let archive = w.archive(&cancel).unwrap();
    (out, archive)
}

#[test]
fn a_layout_3_archive_samples_the_same_values_as_layout_2() {
    // Both archives are opened from their served metadata, not preset.
    let (two, two_layout) = sample_opened(fixture());
    let three_objects = fixture_with(Dtype::F32, &ELEVEN);
    let root: serde_json::Value = serde_json::from_slice(&three_objects["zarr.json"]).unwrap();
    assert_eq!(root["attributes"]["historysyncer_layout_version"], 3);
    let (three, three_layout) = sample_opened(three_objects);
    assert_eq!(
        (two_layout.data.bytes_per_value, two_layout.data.shape[1]),
        (2, 7)
    );
    assert_eq!(
        (
            three_layout.data.bytes_per_value,
            three_layout.data.shape[1]
        ),
        (4, 11)
    );
    assert_eq!(three_layout.data.parameters, vec![1, 3, 4, 5, 6, 7, 8]);
    assert!(two[0].wind.is_some() && two[0].waves.is_some() && two[0].current.is_some());
    assert_eq!(two, three);
}

fn field_first_fixture(dtype: Dtype) -> BTreeMap<String, Vec<u8>> {
    let mut objects = fixture_with(dtype, &ELEVEN);
    objects.retain(|key, _| !key.starts_with("data/"));
    let dtype = if dtype == Dtype::F16 {
        "float16"
    } else {
        "float32"
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "tests/fixtures/whirlwind/field-time-zstd22-{dtype}"
    ));
    objects.insert(
        "data/zarr.json".into(),
        std::fs::read(root.join("zarr.json")).unwrap(),
    );
    for t in 0..2 {
        for y in 0..2 {
            for x in 0..2 {
                let key = format!("c/0/{t}/{y}/{x}");
                objects.insert(
                    format!("data/{key}"),
                    std::fs::read(root.join(key)).unwrap(),
                );
            }
        }
    }
    objects
}

#[test]
fn field_first_zstd22_archives_sample_over_s3_and_from_a_folder() {
    for dtype in [Dtype::F16, Dtype::F32] {
        let objects = field_first_fixture(dtype);
        let dir = folder(&format!("field-first-{dtype:?}"));
        testing::write_fixture_objects(&dir, &objects);
        let local = folder_reader(&dir);
        let server = serve(objects, 206);
        let w = testing::whirlwind_opening(server.endpoint(), 4, BlockCache::new(16 << 20));
        let cancel = Arc::new(AtomicBool::new(false));
        // The first stencil crosses a time shard and longitude tile; the
        // second uses time shard 1 and crosses the latitude shard boundary.
        let points = [
            Point {
                t: 946684800 + 5400,
                lat: 45.,
                lon: -112.5,
            },
            Point {
                t: 946684800 + 9000,
                lat: 15.,
                lon: 22.5,
            },
        ];
        assert!(w.estimate(&points).unwrap().hourly_bytes > 0);
        let sampled = w.sample(&points, &options(), &cancel).unwrap();
        assert_eq!(sampled, local.sample(&points, &options(), &cancel).unwrap());
        let wind = sampled[1].wind.unwrap();
        assert_eq!((wind.u, wind.v), (7.0625, 6.1875));
        let current = sampled[1].current.unwrap();
        assert_eq!((current.u, current.v), (0.7734375, -0.5234375));
        let waves = sampled[1].waves.unwrap();
        assert_eq!((waves.hs, waves.period_s), (Some(4.96875), Some(10.1875)));
        assert!((waves.from.unwrap() % 360.).abs() < 1e-5);
        let before = w.net_totals();
        assert_eq!(w.sample(&points, &options(), &cancel).unwrap(), sampled);
        let estimate = w.estimate(&points).unwrap();
        assert_eq!(estimate.hourly_bytes, 0);
        assert!(estimate.hourly_cached_bytes > 0);
        assert_eq!(w.net_totals(), before);
        assert_eq!(local.net_totals(), (0, 0));
        let requests = server.requests.lock().unwrap();
        let shard_requests: Vec<_> = requests
            .iter()
            .filter(|(key, _)| key.starts_with("data/c/"))
            .collect();
        assert!(
            shard_requests
                .iter()
                .any(|(key, _)| key.starts_with("data/c/0/1/"))
        );
        assert!(
            shard_requests
                .iter()
                .all(|(_, range)| range.starts_with("bytes="))
        );
        drop(requests);

        // Check every cell against the independent fixture formulas, including
        // unused fields preceding u10, every inner tile, and both time shards.
        let archive = w.archive(&cancel).unwrap();
        assert_eq!(archive.data.shape, [4, 11, 4, 8]);
        assert_eq!(archive.data.inner, [2, 11, 2, 2]);
        assert_eq!(archive.data.parameter_stride(), 8);
        let mut plan = ChunkPlan::new();
        for t in 0..4 {
            for y in 0..4 {
                for x in 0..8 {
                    let cell = (t, y, x);
                    plan.entry(archive.data.locate(cell).0)
                        .or_default()
                        .insert(cell);
                }
            }
        }
        let values = w.runtime.block_on(w.read(&archive, plan, &cancel)).unwrap();
        assert_eq!(values.len(), 128);
        for ((t, y, x), actual) in values {
            let (t, y, x) = (t as f32, y as f32, x as f32);
            assert_eq!(
                actual,
                [
                    2. * t + y + x / 8.,
                    4. + t / 2. + y / 4. + x / 8.,
                    0.5 + t / 16. + y / 32. + x / 64.,
                    -0.25 - t / 16. - y / 32. - x / 64.,
                    if t % 2. == 0. { 350. } else { 10. },
                    2. + t + y / 8. + x / 16.,
                    8. + t / 2. + y / 4. + x / 8.,
                ],
                "{dtype:?}, ({t}, {y}, {x})"
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn literal_field_time_y_x_dimension_names_select_the_corresponding_coordinates() {
    let mut objects = field_first_fixture(Dtype::F32);
    let mut data: Value = serde_json::from_slice(&objects["data/zarr.json"]).unwrap();
    data["dimension_names"] = serde_json::json!(["field", "time", "y", "x"]);
    objects.insert("data/zarr.json".into(), serde_json::to_vec(&data).unwrap());
    for (old, new) in [("param", "field"), ("latitude", "y"), ("longitude", "x")] {
        for suffix in ["zarr.json", "c/0"] {
            let bytes = objects.remove(&format!("{old}/{suffix}")).unwrap();
            objects.insert(format!("{new}/{suffix}"), bytes);
        }
    }
    let (out, _) = sample_opened(objects);
    // Hour 0.5, row 0.5, column 20/9.
    let wind = out[0].wind.unwrap();
    assert!((wind.u - (1.5 + 5. / 18.)).abs() < 1e-5);
    assert!(out[0].waves.is_some() && out[0].current.is_some());
}

#[test]
fn field_first_metadata_rejects_ambiguous_axes_and_inconsistent_shapes() {
    let objects = field_first_fixture(Dtype::F32);
    let metadata: Value = serde_json::from_slice(&objects["data/zarr.json"]).unwrap();
    for names in [
        serde_json::json!(["param", "time", "longitude", "latitude"]),
        serde_json::json!(["time", "time", "latitude", "longitude"]),
        serde_json::json!(["time", "latitude", "param", "longitude"]),
        serde_json::json!(["param", "time", "latitude"]),
        Value::Null,
    ] {
        let mut broken = metadata.clone();
        broken["dimension_names"] = names;
        assert!(Array::parse(&broken, (0..7).collect()).is_err());
    }
    for pointer in [
        "/shape/0",
        "/codecs/0/configuration/chunk_shape/0",
        "/chunk_grid/configuration/chunk_shapes/0",
        "/chunk_grid/configuration/chunk_shapes/1",
    ] {
        let mut broken = metadata.clone();
        *broken.pointer_mut(pointer).unwrap() = serde_json::json!(3);
        assert!(
            Array::parse(&broken, (0..7).collect()).is_err(),
            "{pointer}"
        );
    }
}

#[test]
fn historysyncer_v4_presets_read_transposed_tiles_without_transposing_shard_keys() {
    for routing in [false, true] {
        let objects = testing::historysyncer_fixture(routing);
        let dir = folder(if routing {
            "historysyncer-routing"
        } else {
            "historysyncer-base"
        });
        testing::write_fixture_objects(&dir, &objects);
        let local = folder_reader(&dir);
        let server = serve(objects, 206);
        let w = testing::whirlwind_opening(server.endpoint(), 4, BlockCache::new(16 << 20));
        let cancel = Arc::new(AtomicBool::new(false));
        let at = |hour: f64, lon| Point {
            t: 946684800 + (hour * 3600.) as i64,
            lat: 39.25,
            lon,
        };
        let points = [
            at(73., 96.75),
            at(72.5, 96.75),
            at(71.5, 96.75),
            at(73., 99.75),  // marine fields are missing, wind is present
            at(73., 101.75), // all fields missing in a stored tile
            at(73., 91.75),  // an absent tile's index entry
        ];
        assert!(w.estimate(&points).unwrap().hourly_bytes > 0);
        let out = w.sample(&points, &options(), &cancel).unwrap();
        assert_eq!(out, local.sample(&points, &options(), &cancel).unwrap());
        let wind = out[0].wind.unwrap();
        let current = out[0].current.unwrap();
        let waves = out[0].waves.unwrap();
        assert_eq!(
            (wind.u, wind.v),
            if routing {
                (22.5, 14.25)
            } else {
                (22.625, 14.3125)
            }
        );
        assert_eq!(
            (current.u, current.v),
            if routing {
                (1.25, -1.)
            } else {
                (1.3359375, -1.0859375)
            }
        );
        assert_eq!(
            (waves.hs, waves.period_s),
            if routing {
                (Some(7.5), Some(10.75))
            } else {
                (Some(7.375), Some(10.6875))
            }
        );
        assert!((waves.from.unwrap() - 10.).abs() < 1e-5);
        assert_eq!(out[1].wind.unwrap().u, 22.5);
        assert_eq!(
            out[1].waves.unwrap().hs,
            Some(if routing { 7.375 } else { 7.34375 })
        );
        assert_eq!(out[2].wind.unwrap().u, 22.25);
        assert_eq!(out[2].wind.unwrap().v, 14.125);
        assert!(out[3].wind.is_some());
        assert!(out[3].current.is_none() && out[3].waves.is_none());
        assert_eq!(out[4..], [EnvPoint::default(); 2]);
        let before = w.net_totals();
        assert_eq!(w.sample(&points, &options(), &cancel).unwrap(), out);
        assert_eq!(w.estimate(&points).unwrap().hourly_bytes, 0);
        assert_eq!(w.net_totals(), before);
        let archive = w.archive(&cancel).unwrap();
        assert_eq!(archive.data.inner, [72, 11, 20, 20]);
        assert_eq!(archive.data.bytes_per_value, 2);
        assert_eq!(archive.data.parameters, (0..7).collect::<Vec<_>>());
        let (chunk, offset) = archive.data.locate((73, 203, 1107));
        assert_eq!(archive.data.key(chunk), "data/c/1/0/1/3");
        assert_eq!(chunk.inner, 135);
        assert_eq!(offset, 467); // byte storage is field, time, y, x
        assert_eq!(archive.data.parameter_stride(), 72 * 20 * 20);
        let requests = server.requests.lock().unwrap();
        let shards: BTreeSet<_> = requests
            .iter()
            .filter(|(key, _)| key.starts_with("data/c/"))
            .map(|(key, range)| {
                assert!(range.starts_with("bytes="));
                key.as_str()
            })
            .collect();
        assert_eq!(shards, BTreeSet::from(["data/c/0/0/1/3", "data/c/1/0/1/3"]));
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn historysyncer_transpose_and_quantization_must_be_understood_before_sampling() {
    let objects = testing::historysyncer_fixture(true);
    let metadata: Value = serde_json::from_slice(&objects["data/zarr.json"]).unwrap();
    for (pointer, value) in [
        (
            "/codecs/0/configuration/codecs/0/configuration/order",
            serde_json::json!([1, 2, 3, 0]),
        ),
        (
            "/codecs/0/configuration/codecs/0/configuration/order",
            serde_json::json!([1, 1, 2, 3]),
        ),
        (
            "/codecs/0/configuration/codecs/1/configuration/endian",
            serde_json::json!("big"),
        ),
        (
            "/codecs/0/configuration/codecs/2/name",
            serde_json::json!("numcodecs.pcodec"),
        ),
        ("/attributes/quantization/scale", serde_json::json!(2)),
        ("/attributes/quantization/offset", serde_json::json!(1)),
        (
            "/attributes/quantization/already_applied_to_stored_values",
            serde_json::json!(false),
        ),
        (
            "/attributes/quantization/source_dtype",
            serde_json::json!("int16"),
        ),
        ("/fill_value", serde_json::json!("NaN")),
    ] {
        let mut broken = metadata.clone();
        *broken.pointer_mut(pointer).unwrap() = value;
        assert!(
            Array::parse(&broken, (0..7).collect()).is_err(),
            "{pointer}"
        );
    }
}

#[test]
fn a_parameter_axis_that_disagrees_with_the_data_is_refused_naming_both_numbers() {
    let mut objects = fixture_with(Dtype::F32, &ELEVEN);
    let mut data: serde_json::Value = serde_json::from_slice(&objects["data/zarr.json"]).unwrap();
    data["shape"][1] = serde_json::json!(12);
    objects.insert("data/zarr.json".into(), serde_json::to_vec(&data).unwrap());
    let server = serve(objects, 206);
    let w = testing::whirlwind_opening(server.endpoint(), 4, BlockCache::new(16 << 20));
    let cancel = Arc::new(AtomicBool::new(false));
    let err = w.archive(&cancel).unwrap_err().to_string();
    assert!(
        err.contains("11 parameter names") && err.contains("12 parameters"),
        "{err}"
    );
}

fn parameter_axis(names: &[&str]) -> (serde_json::Value, Vec<u8>) {
    let metadata = serde_json::json!({
        "zarr_format":3,"node_type":"array","shape":[names.len()],
        "data_type":{"name":"fixed_length_utf32","configuration":{"length_bytes":56}},
        "chunk_key_encoding":{"name":"default","configuration":{"separator":"/"}},
        "chunk_grid":{"name":"regular","configuration":{"chunk_shape":[names.len()]}},
        "codecs":[{"name":"bytes","configuration":{"endian":"little"}},{"name":"zstd"}]
    });
    let mut raw = vec![0u8; names.len() * 56];
    for (slot, name) in names.iter().enumerate() {
        for (i, ch) in name.chars().enumerate() {
            raw[slot * 56 + i * 4..slot * 56 + i * 4 + 4]
                .copy_from_slice(&(ch as u32).to_le_bytes());
        }
    }
    (metadata, zstd_raw(&raw))
}

#[test]
fn an_archive_missing_a_needed_parameter_is_refused_by_name() {
    let needed = [
        "u10",
        "v10",
        "ucur",
        "vcur",
        "wave_direction",
        "wave_height",
        "wave_period",
    ];
    let names: Vec<&str> = ELEVEN
        .iter()
        .copied()
        .filter(|n| *n != "wave_period")
        .collect();
    let (metadata, bytes) = parameter_axis(&names);
    let err = layout::parameters(&metadata, &bytes, &needed).unwrap_err();
    assert!(
        err.to_string().contains("lacks parameter wave_period"),
        "{err}"
    );
    let (metadata, bytes) = parameter_axis(&ELEVEN);
    assert_eq!(
        layout::parameters(&metadata, &bytes, &needed).unwrap(),
        vec![1, 3, 4, 5, 6, 7, 8]
    );
}

fn sharded(data_type: &str, inner: [usize; 4]) -> serde_json::Value {
    serde_json::json!({
        "zarr_format":3,"node_type":"array","shape":[inner[0],11,inner[2],inner[3]],
        "data_type":data_type,"fill_value":"NaN","dimension_names":["time","param","latitude","longitude"],
        "chunk_key_encoding":{"name":"default","configuration":{"separator":"/"}},
        "chunk_grid":{"name":"rectilinear","configuration":{"kind":"inline","chunk_shapes":[inner[0],11,[inner[2]],[inner[3]]]}},
        "codecs":[{"name":"sharding_indexed","configuration":{
            "chunk_shape":inner,"index_location":"end",
            "codecs":[{"name":"bytes","configuration":{"endian":"little"}},{"name":"zstd","configuration":{"level":5}}],
            "index_codecs":[{"name":"bytes","configuration":{"endian":"little"}},{"name":"crc32c"}]
        }}]
    })
}

#[test]
fn the_inner_chunk_bound_counts_values_not_bytes() {
    let parameters = vec![1, 3, 4, 5, 6, 7, 8];
    let ok = Array::parse(&sharded("float32", [72, 11, 40, 80]), parameters.clone()).unwrap();
    assert_eq!(ok.decoded_len(), 72 * 11 * 40 * 80 * 4);
    assert!(Array::parse(&sharded("float32", [96, 11, 80, 80]), parameters).is_err());
}

#[test]
fn a_float64_archive_is_refused() {
    assert!(
        Array::parse(
            &sharded("float64", [2, 11, 2, 2]),
            vec![1, 3, 4, 5, 6, 7, 8]
        )
        .is_err()
    );
}

fn folder(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("pe-whirlwind-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn folder_reader(dir: &std::path::Path) -> Whirlwind {
    let reference = ArchiveRef {
        identity: format!("file://{}", dir.display()),
        location: testing::file_location(dir),
    };
    Whirlwind::new(
        reference,
        Duration::from_secs(5),
        BlockCache::new(16 << 20),
        4,
    )
    .unwrap()
}

#[test]
fn a_folder_archive_samples_like_the_served_one() {
    let dir = folder("samples");
    testing::write_fixture(&dir);
    let server = serve(fixture(), 206);
    let served = testing::whirlwind_opening(server.endpoint(), 4, BlockCache::new(16 << 20));
    let local = folder_reader(&dir);
    let cancel = Arc::new(AtomicBool::new(false));
    let from_folder = local.sample(&[point()], &options(), &cancel).unwrap();
    let from_server = served.sample(&[point()], &options(), &cancel).unwrap();
    assert_eq!(from_folder, from_server);
    assert_eq!(from_folder[0].wind.unwrap().u, 1.5);
    assert_eq!(local.net_totals(), (0, 0));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_missing_shard_file_is_missing_data_and_a_vanished_folder_fails_the_open_with_its_path() {
    let dir = folder("missing");
    testing::write_fixture(&dir);
    let local = folder_reader(&dir);
    let cancel = AtomicBool::new(false);
    let missing = local
        .runtime
        .block_on(local.transport.get("data/c/9/9/9/9", None, &cancel))
        .unwrap();
    assert!(missing.is_none());
    let past = local.runtime.block_on(local.transport.get(
        "zarr.json",
        Some(Range::Bytes(1 << 20, 4)),
        &cancel,
    ));
    assert!(past.unwrap_err().to_string().contains("zarr.json"));
    std::fs::remove_dir_all(&dir).unwrap();
    let error = local
        .runtime
        .block_on(local.transport.get("zarr.json", None, &cancel))
        .unwrap_err();
    assert!(error.to_string().contains(&dir.display().to_string()));
    let reference = ArchiveRef {
        identity: "file".into(),
        location: testing::file_location(&dir),
    };
    let error = Whirlwind::new(
        reference,
        Duration::from_secs(5),
        BlockCache::new(1 << 20),
        4,
    )
    .unwrap_err();
    assert!(error.to_string().contains(&dir.display().to_string()));
}

#[test]
fn folder_archives_use_memory_but_never_the_disk_cache() {
    let dir = folder("nocache");
    testing::write_fixture(&dir);
    let cache_dir = folder("nocache-disk");
    let disk = DiskCache::open(&cache_dir, 16 << 20).unwrap();
    let local = folder_reader(&dir).with_disk_cache(Arc::clone(&disk));
    let cancel = Arc::new(AtomicBool::new(false));
    let out = local.sample(&[point()], &options(), &cancel).unwrap();
    assert!(out[0].wind.is_some());
    assert_eq!(disk.size().unwrap(), 0);
    // The second sample is answered from memory.
    let before = local.decoded_chunks.load(Ordering::SeqCst);
    assert_eq!(local.sample(&[point()], &options(), &cancel).unwrap(), out);
    assert_eq!(local.decoded_chunks.load(Ordering::SeqCst), before);
    drop(local);
    drop(disk);
    std::fs::remove_dir_all(dir).unwrap();
    std::fs::remove_dir_all(cache_dir).unwrap();
}
