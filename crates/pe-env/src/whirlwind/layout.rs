//! The verified HistorySyncer v3 layout, read from metadata rather than guessed.
use crate::dataset::Cell;
use crate::grid::{Axis, Grid};
use crate::store::TimeAxis;
use crate::{EnvError, Result};
use serde_json::Value;
use std::io::Read;

pub(super) fn bad(message: impl Into<String>) -> EnvError {
    EnvError::Layout(message.into())
}

pub(super) fn decode(bytes: &[u8], expected: usize) -> Result<Vec<u8>> {
    let decoder = ruzstd::decoding::StreamingDecoder::new(bytes)
        .map_err(|e| bad(format!("Whirlwind Zstandard: {e}")))?;
    let mut raw = Vec::new();
    decoder
        .take(expected as u64 + 1)
        .read_to_end(&mut raw)
        .map_err(|e| bad(format!("Whirlwind Zstandard: {e}")))?;
    if raw.len() != expected {
        return Err(bad("Whirlwind chunk has the wrong decoded size"));
    }
    Ok(raw)
}

fn sizes(value: &Value) -> Result<Vec<usize>> {
    value
        .as_array()
        .ok_or_else(|| bad("missing array shape"))?
        .iter()
        .map(|v| {
            v.as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .filter(|n| *n > 0 && *n <= 2_000_000)
                .ok_or_else(|| bad("invalid dimension"))
        })
        .collect()
}

fn codecs(value: &Value) -> bool {
    let Some(c) = value.as_array() else {
        return false;
    };
    c.len() == 2
        && c[0]["name"] == "bytes"
        && c[0]["configuration"]["endian"] == "little"
        && c[1]["name"] == "zstd"
}

fn v3(value: &Value) -> bool {
    value["zarr_format"] == 3
        && value["node_type"] == "array"
        && value["chunk_key_encoding"]["name"] == "default"
        && value["chunk_key_encoding"]["configuration"]["separator"] == "/"
        && value
            .get("storage_transformers")
            .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty))
}

/// Validates the single-chunk coordinate codec and expected length.
pub(super) fn coordinate(
    meta: &Value,
    bytes: &[u8],
    n: usize,
    dtype: &str,
    width: usize,
) -> Result<Vec<u8>> {
    if !v3(meta)
        || sizes(&meta["shape"])? != [n]
        || meta["data_type"] != dtype
        || meta["chunk_grid"]["name"] != "regular"
        || sizes(&meta["chunk_grid"]["configuration"]["chunk_shape"])? != [n]
        || !codecs(&meta["codecs"])
    {
        return Err(bad("unsupported Whirlwind coordinate encoding"));
    }
    decode(bytes, n * width)
}

/// How many names the `param` axis lists. Layout 3 lists more than are needed,
/// so the caller must hold it equal to the data array's parameter dimension.
pub(super) fn axis_len(meta: &Value) -> Result<usize> {
    match sizes(&meta["shape"])?.as_slice() {
        [n] if (1..=256).contains(n) => Ok(*n),
        _ => Err(bad("unsupported Whirlwind parameter encoding")),
    }
}

pub(super) fn parameters(meta: &Value, bytes: &[u8], expected: &[&str]) -> Result<Vec<usize>> {
    let count = axis_len(meta)?;
    let width = meta["data_type"]["configuration"]["length_bytes"]
        .as_u64()
        .unwrap_or(0) as usize;
    if !v3(meta)
        || meta["data_type"]["name"] != "fixed_length_utf32"
        || width == 0
        || width > 256
        || !width.is_multiple_of(4)
        || !codecs(&meta["codecs"])
        || meta["chunk_grid"]["name"] != "regular"
        || sizes(&meta["chunk_grid"]["configuration"]["chunk_shape"])? != [count]
    {
        return Err(bad("unsupported Whirlwind parameter encoding"));
    }
    let raw = decode(bytes, count * width)?;
    let names = raw
        .chunks_exact(width)
        .map(|part| {
            part.chunks_exact(4)
                .map(|v| char::from_u32(u32::from_le_bytes([v[0], v[1], v[2], v[3]])))
                .collect::<Option<String>>()
                .map(|s| s.trim_end_matches('\0').to_owned())
                .ok_or_else(|| bad("invalid parameter name"))
        })
        .collect::<Result<Vec<_>>>()?;
    expected
        .iter()
        .map(|name| {
            names
                .iter()
                .position(|s| s == name)
                .ok_or_else(|| bad(format!("Whirlwind lacks parameter {name}")))
        })
        .collect()
}

#[derive(Debug)]
pub(super) struct Array {
    pub shape: [usize; 4],
    pub inner: [usize; 4],
    pub bands: [Vec<usize>; 2],
    pub parameters: Vec<usize>,
    /// Two for layout 2's float16, four for layout 3's float32.
    pub bytes_per_value: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Chunk {
    pub time: usize,
    pub lat: usize,
    pub lon: usize,
    pub inner: u32,
}

impl Chunk {
    pub fn key(self) -> String {
        format!("data/c/{}/0/{}/{}", self.time, self.lat, self.lon)
    }
}

impl Array {
    pub fn parse(meta: &Value, parameters: Vec<usize>) -> Result<Self> {
        let shape: [usize; 4] = sizes(&meta["shape"])?
            .try_into()
            .map_err(|_| bad("Whirlwind needs four dimensions"))?;
        let shard = &meta["codecs"][0]["configuration"];
        let inner: [usize; 4] = sizes(&shard["chunk_shape"])?
            .try_into()
            .map_err(|_| bad("invalid inner chunk shape"))?;
        let grid = &meta["chunk_grid"]["configuration"]["chunk_shapes"];
        let bands = [sizes(&grid[2])?, sizes(&grid[3])?];
        let bytes_per_value = match meta["data_type"].as_str() {
            Some("float16") => 2,
            Some("float32") => 4,
            _ => return Err(bad("unsupported Whirlwind sharded array layout")),
        };
        if !v3(meta)
            || meta["fill_value"] != "NaN"
            || meta["dimension_names"]
                != serde_json::json!(["time", "param", "latitude", "longitude"])
            || meta["chunk_grid"]["name"] != "rectilinear"
            || meta["chunk_grid"]["configuration"]["kind"] != "inline"
            || meta["codecs"].as_array().map(Vec::len) != Some(1)
            || meta["codecs"][0]["name"] != "sharding_indexed"
            || !codecs(&shard["codecs"])
            || shard["index_location"] != "end"
            || shard["index_codecs"]
                != serde_json::json!([{"name":"bytes","configuration":{"endian":"little"}},{"name":"crc32c"}])
            || grid[0].as_u64() != Some(inner[0] as u64)
            || grid[1].as_u64() != Some(shape[1] as u64)
            || inner[1] != shape[1]
            || parameters.len() != 7
            || parameters.iter().any(|&p| p >= shape[1])
            || inner
                .iter()
                .try_fold(1usize, |n, v| n.checked_mul(*v))
                .is_none_or(|n| n > 4 << 20)
            || bands.iter().enumerate().any(|(i, b)| {
                b.iter().sum::<usize>() != shape[i + 2] || b.iter().any(|n| n % inner[i + 2] != 0)
            })
        {
            return Err(bad("unsupported Whirlwind sharded array layout"));
        }
        Ok(Self {
            shape,
            inner,
            bands,
            parameters,
            bytes_per_value,
        })
    }

    pub fn locate(&self, cell: Cell) -> (Chunk, usize) {
        let (t, y, x) = cell;
        let locate = |v, bands: &[usize]| {
            let mut start = 0;
            for (i, &n) in bands.iter().enumerate() {
                if v < start + n {
                    return (i, v - start);
                }
                start += n;
            }
            unreachable!("cells are bounded by the validated grid")
        };
        let (lat, y) = locate(y, &self.bands[0]);
        let (lon, x) = locate(x, &self.bands[1]);
        let columns = self.bands[1][lon] / self.inner[3];
        let inner = (y / self.inner[2] * columns + x / self.inner[3]) as u32;
        let offset = ((t as usize % self.inner[0]) * self.inner[1] * self.inner[2]
            + y % self.inner[2])
            * self.inner[3]
            + x % self.inner[3];
        (
            Chunk {
                time: t as usize / self.inner[0],
                lat,
                lon,
                inner,
            },
            offset,
        )
    }

    pub fn index_len(&self, chunk: Chunk) -> usize {
        self.bands[0][chunk.lat] / self.inner[2] * (self.bands[1][chunk.lon] / self.inner[3]) * 16
            + 4
    }

    pub fn decoded_len(&self) -> usize {
        self.inner.iter().product::<usize>() * self.bytes_per_value
    }
}

#[derive(Debug)]
pub(super) struct Archive {
    pub data: Array,
    pub grid: Grid,
    pub time: TimeAxis,
}

pub(super) fn grid(lat: &[u8], lon: &[u8]) -> Result<Grid> {
    let axis = |name, bytes: &[u8]| {
        let values = bytes
            .chunks_exact(4)
            .map(|c| f64::from(f32::from_le_bytes([c[0], c[1], c[2], c[3]])))
            .collect::<Vec<_>>();
        Axis::from_values(name, &values)
    };
    Ok(Grid {
        lat: axis("latitude", lat)?,
        lon: axis("longitude", lon)?,
    })
}

pub(super) fn index_entry(bytes: &[u8], chunk: Chunk) -> Result<Option<(u64, u64)>> {
    let n = bytes
        .len()
        .checked_sub(4)
        .ok_or_else(|| bad("truncated shard index"))?;
    let checksum = u32::from_le_bytes([bytes[n], bytes[n + 1], bytes[n + 2], bytes[n + 3]]);
    if crc32c::crc32c(&bytes[..n]) != checksum {
        return Err(bad("Whirlwind shard index checksum failed"));
    }
    let start = chunk.inner as usize * 16;
    let entry = bytes
        .get(start..start + 16)
        .ok_or_else(|| bad("inner chunk lies outside shard index"))?;
    let read = |v: &[u8]| u64::from_le_bytes([v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7]]);
    let (offset, len) = (read(&entry[..8]), read(&entry[8..]));
    if offset == u64::MAX && len == u64::MAX {
        return Ok(None);
    }
    if len == 0 || len > 16 << 20 || offset.checked_add(len).is_none() {
        return Err(bad("invalid inner chunk extent"));
    }
    Ok(Some((offset, len)))
}
