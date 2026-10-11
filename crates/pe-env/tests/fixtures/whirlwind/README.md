`combined-data.zarr.json` is the unmodified metadata from the public S3
archive `whirlwind-hindsight/hindsight-routing/data/zarr.json`, recorded
2026-10-11 after the archive was rewritten with 5° × 5° inner chunks
(20 × 20 cells; 10° × 10° before). It describes the shared seven-parameter
array; the reader tests independently check its shard/inner-chunk offsets
and size. R2 and Tigris held an identical copy of the earlier layout and
hold no archive now. No credentials or weather shard payloads are stored
in this fixture.
