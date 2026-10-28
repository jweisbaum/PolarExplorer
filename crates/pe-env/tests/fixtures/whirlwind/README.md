`combined-data.zarr.json` is the unmodified metadata from the public S3
archive `whirlwind-hindsight/hindsight-routing/data/zarr.json`, recorded
2026-10-11 after the archive was rewritten with 5° × 5° inner chunks
(20 × 20 cells; 10° × 10° before). It describes the shared seven-parameter
array; the reader tests independently check its shard/inner-chunk offsets
and size. R2 and Tigris held an identical copy of the earlier layout and
hold no archive now. No credentials or weather shard payloads are stored
in this fixture.

`field-time-zstd22-float16/` and `field-time-zstd22-float32/` contain synthetic
`data` array metadata and shards, generated independently by
`generate_field_time.py` with the reference Zstandard CLI v1.5.6, using
`--ultra -22 --check`. They store eleven reordered fields in
**field–time–y–x** order, with two time shards, two latitude bands, two
longitude bands, and two inner tiles per shard. The tests supply coordinate
arrays from the existing synthetic archive and read these shards over the
loopback S3 server and from a folder. Values vary across all four axes; tests
check every cell and hand-computed interpolated wind, current and waves.
Tests need neither Python nor the Zstandard CLI.

`historysyncer-v4/` records the current HistorySyncer writer's **Climatology
base** and **Routing** creation presets. `generate_historysyncer.py` builds an
isolated copy of that checkout with the companion Rust harness, calls its
`create_archive_with_options` and `Compression::encode` functions, and copies
the resulting metadata, coordinate arrays, and synthetic shards here. The
writer also decodes its own tiles before they are recorded. Source hashes are
in `writer-sha256.json`; no credentials or downloaded weather are included.

These archives keep logical dimensions and shard keys in time–field–y–x order,
and declare an inner `transpose [1,0,2,3] → bytes → zstd(level=22)` pipeline.
Each tile contains 72 × 11 × 20 × 20 float16 values. Both time shards hold
tiles 135 and 136 in latitude band 1, longitude band 3; other entries are
missing. Tile 135 varies by time, field, row and column, with missing marine
fields in its last column. Tile 136 contains only missing data. Routing's
quarter-unit rounding is already applied, with `-999` as its sentinel;
Climatology base retains float16 values and NaNs. Tests check both presets
over S3 and from folders, temporal interpolation across shards, missing-data
handling, and an app weather job with layout-4 provenance.

To regenerate from the current writer source:

```sh
python3 crates/pe-env/tests/fixtures/whirlwind/generate_historysyncer.py /path/to/HistorySyncer
```
