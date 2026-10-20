//! Reanalysis GRIB export (spec.md 7.8): the 10 m wind over a track's area
//! and times, and optionally its waves and current, written to a `.grib2`
//! file as a cancellable job.
//!
//! **What is written.** The area is the track's bounding box plus 2°, on
//! ERA5's native 0.25° grid (`pe_grib::region`); the times every hour (or
//! every third) from the first fix to the last. Each time is a group of
//! messages in a fixed order: wind u and v, then wave height and direction,
//! then current u and v, each ticked part present at every time. Values are
//! in the archives' units (m/s, m, degrees "from"), as GRIB wants them.
//!
//! **Where it comes from.** The same [`Provider`] as a track's fetch, asked
//! at every node of the grid and every time: at a node and whole hour the
//! values are the archive's own (the stencil lands on the node). A node
//! with no value — land for waves and current, an hour the archive does not
//! have — is written as missing (a GRIB bitmap), never as zero. Only the
//! inner chunks holding the area are downloaded (spec.md 7.5), into the
//! session's memory and the bounded disk cache.
//!
//! **In what order.** The current is read first, tile by tile (16 × 16
//! nodes over every time), and spooled to a temporary file beside the
//! export; then the wind and waves hour by hour, a few hours per read, each
//! hour's messages written as soon as its values are in. Neither the file
//! nor the current is ever held whole in memory.
//!
//! **Atomic and reproducible.** The file is written to `<name>.tmp` and
//! renamed into place when complete; a cancel or a failure removes it and
//! leaves whatever was at the path. The same track, options and archive
//! bytes give the same file on every platform (invariant 5,
//! `tests/grib_export.rs`).

use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use pe_core::SourceId;
use pe_env::{EnvError, EnvPoint, Interval, Options, Parts, Point, Provider};
use pe_grib::{GribFile, GridSpec, MessageSpec, Parameter, ReferenceTime};
use serde::Serialize;
use ts_rs::TS;

use crate::commands::AppState;
use crate::error::{AppError, Context, Result};

/// The event carrying [`GribExportStatus`] whenever an export starts,
/// advances or ends.
pub const PROGRESS_EVENT: &str = "grib://progress";

/// Nodes per side of a current tile: 4° at 0.25°, so a tile's reads over
/// every time stay small.
const TILE: u32 = 16;

/// The most positions one read asks for (about 16 MB of answers); a
/// larger area is read in runs of nodes.
const MAX_POINTS: usize = 200_000;

/// At most this many times per wind-and-wave read: enough requests in
/// flight to hide the archive's latency, few enough that progress moves
/// and a cancel loses little.
const MAX_TIMES_PER_READ: usize = 6;

// ---------------------------------------------------------------- plan

/// What one export writes.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// The area.
    pub grid: GridSpec,
    /// The times, epoch seconds, in order.
    pub times: Vec<i64>,
    /// What is written: the wind always, waves and current if ticked.
    pub parts: Parts,
    /// The spacing of the times.
    pub interval: Interval,
    /// The project's Stokes drift choice, passed on as for a fetch.
    pub stokes_drift: bool,
}

impl Plan {
    /// The plan for a track's positions `(t, lat, lon)`; `None` without
    /// one.
    pub fn of(
        fixes: &[(i64, f64, f64)],
        waves: bool,
        current: bool,
        interval: Interval,
        stokes_drift: bool,
    ) -> Option<Self> {
        let positions: Vec<(f64, f64)> = fixes.iter().map(|(_, lat, lon)| (*lat, *lon)).collect();
        let grid = pe_grib::region(&positions, pe_grib::MARGIN_DEG, pe_grib::NATIVE_STEP)?;
        let first = fixes.iter().map(|f| f.0).min()?;
        let last = fixes.iter().map(|f| f.0).max()?;
        let times = pe_grib::times(first, last, interval.seconds());
        Some(Self {
            grid,
            times,
            parts: Parts {
                wind: true,
                waves,
                current,
            },
            interval,
            stokes_drift,
        })
    }

    /// The parameters written at each time, in order.
    pub fn parameters(&self) -> Vec<Parameter> {
        let mut out = vec![Parameter::WindU, Parameter::WindV];
        if self.parts.waves {
            out.extend([Parameter::WaveHeight, Parameter::WaveDirection]);
        }
        if self.parts.current {
            out.extend([Parameter::CurrentU, Parameter::CurrentV]);
        }
        out
    }

    /// Messages in the file.
    pub fn messages(&self) -> u64 {
        self.times.len() as u64 * self.parameters().len() as u64
    }

    /// About the file's size: each message's fixed sections (209 bytes) and
    /// two bytes a node, plus a bitmap for the waves and current (which
    /// have land).
    pub fn file_bytes(&self) -> u64 {
        let nodes = self.grid.point_count();
        let fixed = 209 + 2 * nodes;
        let bitmap = nodes.div_ceil(8) + 1;
        let with_bitmap = self.parameters().len() as u64 - 2;
        self.times.len() as u64 * (2 * fixed + with_bitmap * (fixed + bitmap))
    }
}

/// The grid's nodes in scanning order, `(lat, lon)` in the app's range.
fn nodes(grid: GridSpec) -> Vec<(f64, f64)> {
    grid.points().map(|(lon, lat)| (lat, lon)).collect()
}

// ------------------------------------------------------------ estimate

/// What an export will write and download, for the dialog (spec.md 7.8,
/// 13).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "GribPreview.ts")]
pub struct GribPreview {
    /// The track source.
    pub source_id: u64,
    /// Its label.
    pub label: String,
    /// The area's northern row, degrees.
    pub north: f64,
    /// The area's southern row, degrees.
    pub south: f64,
    /// The area's western column, degrees in [−180, 180).
    pub west: f64,
    /// The area's eastern column, degrees in [−180, 180).
    pub east: f64,
    /// Columns.
    pub ni: u32,
    /// Rows.
    pub nj: u32,
    /// Times written.
    pub times: u32,
    /// The first time, epoch seconds.
    pub first: i64,
    /// The last time, epoch seconds.
    pub last: i64,
    /// Messages in the file.
    pub messages: u32,
    /// Bytes to download. The archive keeps wind, waves and current in one
    /// chunk, so this is every part's download.
    pub wind_bytes: u64,
    /// Always 0: the waves come in the wind's chunks.
    pub waves_bytes: u64,
    /// Always 0: the current comes in the wind's chunks.
    pub current_bytes: u64,
    /// Bytes already held in memory or on disk.
    pub cached_bytes: u64,
    /// About the file's size with the parts ticked.
    pub file_bytes: u64,
    /// The current's temporary file beside the export (0 without the
    /// current).
    pub spool_bytes: u64,
    /// Above this the dialog warns about the temporary file's size.
    pub spool_warning_bytes: u64,
}

/// Bytes of the current's temporary file for `plan`: two `f32` a node
/// and time.
pub fn spool_bytes(plan: &Plan) -> u64 {
    plan.grid.point_count() * plan.times.len() as u64 * 8
}

/// The temporary file size above which the dialog warns about disk space.
pub const SPOOL_WARNING_BYTES: u64 = 1_000_000_000;

/// The longest gap between the nodes [`preview_points`] keeps, in nodes:
/// 1° at 0.25°, well under an archive inner chunk's 20 cells.
const PREVIEW_NODE_STRIDE: usize = 4;

/// The longest gap between the times [`preview_points`] keeps, in hours:
/// an archive inner chunk's 72.
const PREVIEW_TIME_STRIDE: usize = 72;

/// `0, stride, 2 × stride, …` below `n`, and `n − 1`: no gap wider than
/// `stride`, both ends kept.
fn strided(n: usize, stride: usize) -> Vec<usize> {
    let mut kept: Vec<usize> = (0..n).step_by(stride.max(1)).collect();
    if n > 0 && kept.last() != Some(&(n - 1)) {
        kept.push(n - 1);
    }
    kept
}

/// The positions the preview estimates: the plan's nodes and times, thinned
/// to at most [`MAX_POINTS`] where that can be done structurally. Rows and
/// columns are kept at most [`PREVIEW_NODE_STRIDE`] apart and times at most
/// [`PREVIEW_TIME_STRIDE`] apart, ends included, so every archive inner
/// chunk the full plan touches still has a position in it and the
/// estimate counts it. The finest node stride that fits is used; a plan too
/// large for even the coarsest is estimated over more positions rather
/// than fewer chunks.
fn preview_points(plan: &Plan) -> Vec<Point> {
    let g = plan.grid;
    let (ni, nj, nt) = (g.ni as usize, g.nj as usize, plan.times.len());
    let fits = |s: usize, k: usize| {
        strided(nj, s).len() * strided(ni, s).len() * strided(nt, k).len() <= MAX_POINTS
    };
    let (s, k) = (1..=PREVIEW_NODE_STRIDE)
        .find_map(|s| {
            (1..=PREVIEW_TIME_STRIDE)
                .find(|&k| fits(s, k))
                .map(|k| (s, k))
        })
        .unwrap_or((PREVIEW_NODE_STRIDE, PREVIEW_TIME_STRIDE));
    let (rows, cols) = (strided(nj, s), strided(ni, s));
    strided(nt, k)
        .into_iter()
        .flat_map(|t| {
            let t = plan.times[t];
            let cols = &cols;
            rows.iter().flat_map(move |&j| {
                let lat = g.lat(j as u32).clamp(-90.0, 90.0);
                cols.iter().map(move |&i| Point {
                    t,
                    lat,
                    lon: g.lon(i as u32),
                })
            })
        })
        .collect()
}

/// The dialog's figures for `plan`. The download is `provider`'s exact
/// accounting of the archive chunks the plan's grid nodes need at its
/// times, from [`preview_points`] so a large area costs no more to
/// estimate than one read; what the provider already holds is not counted.
///
/// # Errors
/// The archive's metadata or indexes cannot be read.
pub fn preview_of(
    source_id: u64,
    label: &str,
    plan: &Plan,
    provider: &pe_env::Reanalysis,
) -> Result<GribPreview> {
    let g = plan.grid;
    let points = preview_points(plan);
    let e = provider.estimate(&points).map_err(|err| AppError::Doing {
        doing: "estimate the reanalysis download for",
        what: label.to_owned(),
        why: err.to_string(),
    })?;
    Ok(GribPreview {
        source_id,
        label: label.to_owned(),
        north: g.lat(0),
        south: g.lat(g.nj - 1),
        west: g.lon(0),
        east: g.lon(g.ni - 1),
        ni: g.ni,
        nj: g.nj,
        times: count(plan.times.len()),
        first: plan.times.first().copied().unwrap_or_default(),
        last: plan.times.last().copied().unwrap_or_default(),
        messages: u32::try_from(plan.messages()).unwrap_or(u32::MAX),
        wind_bytes: e.hourly_bytes,
        waves_bytes: 0,
        current_bytes: 0,
        cached_bytes: e.hourly_cached_bytes,
        file_bytes: plan.file_bytes(),
        spool_bytes: if plan.parts.current {
            spool_bytes(plan)
        } else {
            0
        },
        spool_warning_bytes: SPOOL_WARNING_BYTES,
    })
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The chosen interval by name.
fn interval(name: &str) -> Result<Interval> {
    match name {
        "hourly" => Ok(Interval::Hourly),
        other => Err(AppError::BadOption {
            field: "GRIB interval",
            value: other.to_owned(),
        }),
    }
}

/// The plan for a track of the open project, and its label.
pub fn plan_for(
    state: &AppState,
    source_id: u64,
    interval_name: &str,
    waves: bool,
    current: bool,
) -> Result<(Plan, String)> {
    let interval = interval(interval_name)?;
    state.with_session(|session| {
        let open = session.require_open()?;
        let source = open
            .project
            .source(SourceId(source_id))
            .ok_or(AppError::Core(pe_core::CoreError::MissingSource(source_id)))?;
        let track = source.track().ok_or_else(|| AppError::BadOption {
            field: "Track",
            value: source.label.clone(),
        })?;
        let fixes: Vec<(i64, f64, f64)> = track.fixes.iter().map(|f| (f.t, f.lat, f.lon)).collect();
        let plan = Plan::of(
            &fixes,
            waves,
            current,
            interval,
            open.project.blend.include_stokes_drift,
        )
        .ok_or_else(|| AppError::Doing {
            doing: "export the reanalysis GRIB of",
            what: source.label.clone(),
            why: "the track has no positions".to_owned(),
        })?;
        Ok((plan, source.label.clone()))
    })
}

// -------------------------------------------------------------- export

/// How an export ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The file is in place.
    Done(Summary),
    /// Cancelled: no file was written and whatever was at the path is
    /// untouched.
    Cancelled,
}

/// What a finished export wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    /// The file's size.
    pub bytes: u64,
    /// Its messages.
    pub messages: u32,
    /// Messages with no value anywhere (an hour an archive does not have).
    pub empty_messages: u32,
}

/// The current spooled to disk: for each tile, for each time, its nodes'
/// `(u, v)` as little-endian `f32`. Removed when dropped.
struct Spool {
    path: PathBuf,
    file: File,
    /// Each tile's nodes (indices into the grid) and its first byte.
    tiles: Vec<(Vec<usize>, u64)>,
}

impl Drop for Spool {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Spool {
    /// Fills `u` and `v` (the whole grid) at time index `k`.
    fn read(&mut self, k: usize, u: &mut [f32], v: &mut [f32]) -> std::io::Result<()> {
        for (idx, start) in &self.tiles {
            let at = start + (k * idx.len() * 8) as u64;
            let mut bytes = vec![0u8; idx.len() * 8];
            self.file.seek(SeekFrom::Start(at))?;
            self.file.read_exact(&mut bytes)?;
            for (n, &node) in idx.iter().enumerate() {
                let word = |o: usize| {
                    f32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]])
                };
                u[node] = word(n * 8);
                v[node] = word(n * 8 + 4);
            }
        }
        Ok(())
    }
}

/// The grid's tiles of [`TILE`] × [`TILE`] nodes, each its node indices.
fn tiles(grid: GridSpec) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    for j0 in (0..grid.nj).step_by(TILE as usize) {
        for i0 in (0..grid.ni).step_by(TILE as usize) {
            let idx = (j0..(j0 + TILE).min(grid.nj))
                .flat_map(|j| {
                    (i0..(i0 + TILE).min(grid.ni)).map(move |i| (j * grid.ni + i) as usize)
                })
                .collect();
            out.push(idx);
        }
    }
    out
}

/// Reads a batch, turning a read that stopped for the cancel into
/// `Ok(None)`.
fn read_batch(
    provider: &dyn Provider,
    points: &[Point],
    options: &Options,
    cancel: &Arc<AtomicBool>,
) -> Result<Option<Vec<EnvPoint>>> {
    match provider.sample(points, options, cancel) {
        Ok(found) => Ok(Some(found)),
        Err(EnvError::Cancelled) => Ok(None),
        Err(_) if cancel.load(Ordering::SeqCst) => Ok(None),
        Err(err) => Err(AppError::Doing {
            doing: "read the reanalysis for",
            what: "the GRIB export".to_owned(),
            why: err.to_string(),
        }),
    }
}

/// Reads the current tile by tile over every time into a spool beside
/// `path`. `None` when cancelled.
fn spool_current(
    provider: &dyn Provider,
    plan: &Plan,
    path: &Path,
    cancel: &Arc<AtomicBool>,
    progress: &mut dyn FnMut(f64),
) -> Result<Option<Spool>> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".current.tmp");
    let spool_path = path.with_file_name(name);
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&spool_path)
        .doing("write the current's temporary file", spool_path.display())?;
    let mut spool = Spool {
        path: spool_path,
        file,
        tiles: Vec::new(),
    };
    let all = nodes(plan.grid);
    let options = Options {
        interval: plan.interval,
        stokes_drift: plan.stokes_drift,
        parts: Parts {
            wind: false,
            waves: false,
            current: true,
        },
    };
    let tiles = tiles(plan.grid);
    let total = tiles.len().max(1);
    let mut offset = 0u64;
    let mut out = BufWriter::new(&spool.file);
    for (t, idx) in tiles.into_iter().enumerate() {
        let per_read = (MAX_POINTS / idx.len().max(1)).max(1);
        for times in plan.times.chunks(per_read) {
            if cancel.load(Ordering::SeqCst) {
                return Ok(None);
            }
            let all = &all;
            let points: Vec<Point> = times
                .iter()
                .flat_map(|&t| {
                    idx.iter().map(move |&n| Point {
                        t,
                        lat: all[n].0,
                        lon: all[n].1,
                    })
                })
                .collect();
            let Some(found) = read_batch(provider, &points, &options, cancel)? else {
                return Ok(None);
            };
            let mut bytes = Vec::with_capacity(found.len() * 8);
            for env in &found {
                let (u, v) = env
                    .current
                    .map_or((f32::NAN, f32::NAN), |c| (c.u as f32, c.v as f32));
                bytes.extend_from_slice(&u.to_le_bytes());
                bytes.extend_from_slice(&v.to_le_bytes());
            }
            out.write_all(&bytes)
                .doing("write the current's temporary file", spool.path.display())?;
        }
        let len = (idx.len() * plan.times.len() * 8) as u64;
        spool.tiles.push((idx, offset));
        offset += len;
        progress((t + 1) as f64 / total as f64);
    }
    out.flush()
        .doing("write the current's temporary file", spool.path.display())?;
    drop(out);
    Ok(Some(spool))
}

/// Writes `plan` to `path`, reading through `provider`. `progress` gets
/// the fraction done (0–1) as it moves.
///
/// # Errors
/// A read or a write failing; nothing is left at `path` but what was
/// there before, and no temporary file.
pub fn export(
    provider: &dyn Provider,
    plan: &Plan,
    path: &Path,
    cancel: &Arc<AtomicBool>,
    progress: &mut dyn FnMut(f64),
) -> Result<Outcome> {
    let reference = plan
        .times
        .first()
        .copied()
        .ok_or_else(|| AppError::Internal("an export with no times".to_owned()))?;
    let reference_time =
        ReferenceTime::from_epoch(reference).map_err(|e| AppError::Internal(e.to_string()))?;
    let mut file = GribFile::create(path).doing("write the GRIB file", path.display())?;

    // The current first, box by box. It downloads the archive's combined
    // chunks, which the wind and waves then read again from memory, so it
    // counts as half the progress.
    let weight = if plan.parts.current { 0.5 } else { 0.0 };
    let mut spool = if plan.parts.current {
        let mut step = |f: f64| progress(weight * f);
        match spool_current(provider, plan, path, cancel, &mut step)? {
            Some(spool) => Some(spool),
            None => return Ok(Outcome::Cancelled),
        }
    } else {
        None
    };

    let all = nodes(plan.grid);
    let n = all.len();
    let options = Options {
        interval: plan.interval,
        stokes_drift: plan.stokes_drift,
        parts: Parts {
            wind: true,
            waves: plan.parts.waves,
            current: false,
        },
    };
    // Every hour's chunk heads, side by side, before the blocks: the four
    // corners of the area at each time.
    let corners: Vec<Point> = plan
        .times
        .iter()
        .flat_map(|&t| {
            [
                0,
                plan.grid.ni as usize - 1,
                n - plan.grid.ni as usize,
                n - 1,
            ]
            .map(|k| Point {
                t,
                lat: all[k].0,
                lon: all[k].1,
            })
        })
        .collect();
    match provider.prepare(&corners, &options, cancel) {
        _ if cancel.load(Ordering::SeqCst) => return Ok(Outcome::Cancelled),
        Err(EnvError::Cancelled) => return Ok(Outcome::Cancelled),
        Err(err) => {
            return Err(AppError::Doing {
                doing: "read the reanalysis for",
                what: "the GRIB export".to_owned(),
                why: err.to_string(),
            });
        }
        Ok(()) => {}
    }

    let per_read = (MAX_POINTS / n.max(1)).clamp(1, MAX_TIMES_PER_READ);
    let parameters = plan.parameters();
    let mut empty = 0u32;
    let (mut cu, mut cv) = (vec![f32::NAN; n], vec![f32::NAN; n]);
    let mut done = 0usize;
    for (batch, times) in plan.times.chunks(per_read).enumerate() {
        if cancel.load(Ordering::SeqCst) {
            return Ok(Outcome::Cancelled);
        }
        // Each time's wind and waves, `[u, v, height, direction]` a node.
        // An area larger than one read (a global grid is a million nodes)
        // is read in runs of nodes, never more than MAX_POINTS at once.
        let mut fields: Vec<[Vec<f32>; 4]> = times
            .iter()
            .map(|_| std::array::from_fn(|_| vec![f32::NAN; n]))
            .collect();
        let run = (MAX_POINTS / times.len()).max(1);
        for start in (0..n).step_by(run) {
            let nodes = &all[start..(start + run).min(n)];
            let points: Vec<Point> = times
                .iter()
                .flat_map(|&t| nodes.iter().map(move |&(lat, lon)| Point { t, lat, lon }))
                .collect();
            let Some(found) = read_batch(provider, &points, &options, cancel)? else {
                return Ok(Outcome::Cancelled);
            };
            for (field, env) in fields.iter_mut().zip(found.chunks(nodes.len())) {
                for (k, e) in env.iter().enumerate() {
                    let at = start + k;
                    let value = |x: Option<f64>| x.map_or(f32::NAN, |v| v as f32);
                    field[0][at] = value(e.wind.map(|w| w.u));
                    field[1][at] = value(e.wind.map(|w| w.v));
                    field[2][at] = value(e.waves.and_then(|w| w.hs));
                    field[3][at] = value(e.waves.and_then(|w| w.from));
                }
            }
        }
        for (k, (&t, field)) in times.iter().zip(&fields).enumerate() {
            let index = batch * per_read + k;
            if let Some(spool) = spool.as_mut() {
                spool
                    .read(index, &mut cu, &mut cv)
                    .doing("read the current's temporary file", spool.path.display())?;
            }
            let forecast_hour = u32::try_from((t - reference) / 3600)
                .map_err(|_| AppError::Internal("an export over too many hours".to_owned()))?;
            for &parameter in &parameters {
                let values: &[f32] = match parameter {
                    Parameter::WindU => &field[0],
                    Parameter::WindV => &field[1],
                    Parameter::WaveHeight => &field[2],
                    Parameter::WaveDirection => &field[3],
                    Parameter::CurrentU => &cu,
                    Parameter::CurrentV => &cv,
                };
                if values.iter().all(|v| !v.is_finite()) {
                    empty += 1;
                }
                let spec = MessageSpec {
                    parameter,
                    grid: plan.grid,
                    reference_time,
                    forecast_hour,
                    centre: 255,
                    bits: pe_grib::packing::BITS_PER_VALUE,
                };
                file.write(&spec, values)
                    .doing("write the GRIB file", path.display())?;
            }
            done += 1;
        }
        progress(weight + (1.0 - weight) * done as f64 / plan.times.len() as f64);
    }
    drop(spool);
    let written = file.commit().doing("write the GRIB file", path.display())?;
    Ok(Outcome::Done(Summary {
        bytes: written.bytes,
        messages: written.messages,
        empty_messages: empty,
    }))
}

// ------------------------------------------------------------------ job

/// The running or last export, as the dialog shows it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[ts(export_to = "GribExportStatus.ts")]
pub struct GribExportStatus {
    /// The track exported, while running and after.
    pub source_id: Option<u64>,
    /// `"idle"`, `"running"`, `"done"`, `"cancelled"` or `"failed"`.
    pub state: String,
    /// How much is done, 0–1.
    pub fraction: f64,
    /// The file (asked for, or written).
    pub path: Option<String>,
    /// Bytes downloaded so far.
    pub downloaded_bytes: u64,
    /// Its size, once written.
    pub bytes: u64,
    /// Its messages, once written.
    pub messages: u32,
    /// Messages with no value anywhere (an hour an archive does not have).
    pub empty_messages: u32,
    /// What went wrong, for `"failed"`.
    pub message: Option<String>,
    /// A current source that would not open and was left out.
    pub warning: Option<String>,
}

/// The export job: at most one at a time.
#[derive(Debug, Default)]
pub struct GribJobs {
    status: Mutex<GribExportStatus>,
    cancel: Mutex<Arc<AtomicBool>>,
}

impl GribJobs {
    /// The status now.
    pub fn status(&self) -> GribExportStatus {
        let mut status = self.status.lock().map(|s| s.clone()).unwrap_or_default();
        if status.state.is_empty() {
            status.state = "idle".to_owned();
        }
        status
    }

    fn update(&self, f: impl FnOnce(&mut GribExportStatus)) -> GribExportStatus {
        let mut status = self
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut status);
        status.clone()
    }

    /// Claims the job for `source`, refusing while another runs; returns
    /// its cancel flag.
    fn begin(&self, source: u64, path: &str) -> Result<Arc<AtomicBool>> {
        let mut status = self
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if status.state == "running" {
            return Err(AppError::Doing {
                doing: "start a GRIB export to",
                what: path.to_owned(),
                why: "another export is still running".to_owned(),
            });
        }
        *status = GribExportStatus {
            source_id: Some(source),
            state: "running".to_owned(),
            path: Some(path.to_owned()),
            ..GribExportStatus::default()
        };
        let flag = Arc::new(AtomicBool::new(false));
        if let Ok(mut cancel) = self.cancel.lock() {
            *cancel = Arc::clone(&flag);
        }
        Ok(flag)
    }

    /// Asks the running export to stop.
    pub fn cancel(&self) {
        if let Ok(cancel) = self.cancel.lock() {
            cancel.store(true, Ordering::SeqCst);
        }
    }
}

/// Ends a job still "running" when dropped, as failed: what unwinding
/// through [`run`] leaves.
struct Unfinished<'a> {
    jobs: &'a GribJobs,
    sink: &'a dyn GribSink,
}

impl Drop for Unfinished<'_> {
    fn drop(&mut self) {
        if self.jobs.status().state == "running" {
            let status = self.jobs.update(|s| {
                s.state = "failed".to_owned();
                s.message = Some("the export stopped unexpectedly; nothing was written".to_owned());
            });
            self.sink.progress(&status);
        }
    }
}

/// Where an export reports to: the frontend in the app, nothing in tests.
pub trait GribSink: Send + Sync {
    /// The status changed.
    fn progress(&self, status: &GribExportStatus);
}

/// Runs one export to its end on this thread, the job's status kept in
/// `state` and reported to `sink`. `net` reads the bytes downloaded so far.
pub fn run(
    state: &AppState,
    provider: &dyn Provider,
    net: &dyn Fn() -> u64,
    plan: &Plan,
    path: &str,
    cancel: &Arc<AtomicBool>,
    sink: &dyn GribSink,
) -> GribExportStatus {
    let jobs = &state.grib_jobs;
    // Whatever happens below, the job does not stay "running": a panic in
    // the export (a bug) ends it as failed, and the dialog says so.
    let _guard = Unfinished { jobs, sink };
    let before = net();
    let mut last = -1.0f64;
    let mut progress = |fraction: f64| {
        // Every percent, not every read.
        if fraction - last >= 0.01 || fraction >= 1.0 {
            last = fraction;
            let downloaded = net().saturating_sub(before);
            let status = jobs.update(|s| {
                s.fraction = fraction.clamp(0.0, 1.0);
                s.downloaded_bytes = downloaded;
            });
            sink.progress(&status);
        }
    };
    let outcome = export(provider, plan, Path::new(path), cancel, &mut progress);
    let warnings = provider.take_warnings();
    let downloaded = net().saturating_sub(before);
    let status = jobs.update(|s| {
        s.downloaded_bytes = downloaded;
        s.warning = (!warnings.is_empty()).then(|| warnings.join("; "));
        match outcome {
            Ok(Outcome::Done(summary)) => {
                s.state = "done".to_owned();
                s.fraction = 1.0;
                s.bytes = summary.bytes;
                s.messages = summary.messages;
                s.empty_messages = summary.empty_messages;
            }
            Ok(Outcome::Cancelled) => s.state = "cancelled".to_owned(),
            Err(err) => {
                s.state = "failed".to_owned();
                s.message = Some(err.to_string());
            }
        }
    });
    sink.progress(&status);
    status
}

/// [`start_grib_export`]'s checks: the plan, and the job claimed.
pub fn begin(
    state: &AppState,
    source_id: u64,
    path: &str,
    interval_name: &str,
    waves: bool,
    current: bool,
) -> Result<(Plan, Arc<AtomicBool>)> {
    let (plan, _) = plan_for(state, source_id, interval_name, waves, current)?;
    let cancel = state.grib_jobs.begin(source_id, path)?;
    Ok((plan, cancel))
}

// ------------------------------------------------------------------ IPC

/// What exporting a track's reanalysis would write and download.
#[tauri::command(async)]
pub fn grib_preview(
    state: tauri::State<'_, AppState>,
    source_id: u64,
    interval: String,
    waves: bool,
    current: bool,
) -> Result<GribPreview> {
    let (plan, label) = plan_for(&state, source_id, &interval, waves, current)?;
    let provider = crate::env::provider(&state)?;
    preview_of(source_id, &label, &plan, &provider)
}

/// A provider and how to read the bytes it has downloaded.
type Reader = (Arc<dyn Provider>, Arc<dyn Fn() -> u64 + Send + Sync>);

/// The provider an export reads through: the session's.
fn export_provider(state: &AppState) -> Result<Reader> {
    let provider = crate::env::provider(state)?;
    let counter = Arc::clone(&provider);
    Ok((provider, Arc::new(move || counter.net_totals().1)))
}

/// The frontend, as a [`GribSink`].
struct Frontend(tauri::AppHandle);

impl GribSink for Frontend {
    fn progress(&self, status: &GribExportStatus) {
        use tauri::Emitter;
        let _ = self.0.emit(PROGRESS_EVENT, status);
    }
}

/// Starts exporting a track's reanalysis to `path` in the background;
/// progress arrives as [`PROGRESS_EVENT`].
#[tauri::command]
pub fn start_grib_export(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    source_id: u64,
    path: String,
    interval: String,
    waves: bool,
    current: bool,
) -> Result<GribExportStatus> {
    let (provider, net) = export_provider(&state)?;
    let (plan, cancel) = begin(&state, source_id, &path, &interval, waves, current)?;
    let status = state.grib_jobs.status();
    let handle = app.clone();
    let spawned = std::thread::Builder::new()
        .name("grib-export".to_owned())
        .spawn(move || {
            use tauri::Manager;
            let state = handle.state::<AppState>();
            let sink = Frontend(handle.clone());
            run(
                &state,
                provider.as_ref(),
                net.as_ref(),
                &plan,
                &path,
                &cancel,
                &sink,
            );
        });
    if let Err(err) = spawned {
        let failed = state.grib_jobs.update(|s| {
            s.state = "failed".to_owned();
            s.message = Some(err.to_string());
        });
        return Ok(failed);
    }
    use tauri::Emitter;
    let _ = app.emit(PROGRESS_EVENT, &status);
    Ok(status)
}

/// Stops the running export; nothing is written.
#[tauri::command]
pub fn cancel_grib_export(state: tauri::State<'_, AppState>) -> Result<GribExportStatus> {
    state.grib_jobs.cancel();
    Ok(state.grib_jobs.status())
}

/// The export now (for a dialog that just opened).
#[tauri::command]
pub fn grib_export_status(state: tauri::State<'_, AppState>) -> Result<GribExportStatus> {
    Ok(state.grib_jobs.status())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The largest gap between consecutive values of `kept` (sorted,
    /// distinct), and whether it starts at `first` and ends at `last`.
    fn widest_gap<T: Copy + PartialOrd + std::ops::Sub<Output = T>>(
        kept: &[T],
        first: T,
        last: T,
    ) -> Option<T> {
        assert!(kept.first() == Some(&first) && kept.last() == Some(&last));
        kept.windows(2)
            .map(|w| w[1] - w[0])
            .fold(None, |a, d| match a {
                Some(a) if a >= d => Some(a),
                _ => Some(d),
            })
    }

    /// Review round 1: 200 × 200 nodes over 200 hours is 8,000,000
    /// positions. A flat stride of 40 kept only columns 0, 40, 80, …, so
    /// 20-cell chunks between were never counted. The structural thinning
    /// keeps every row and column (no gap over 4 nodes, under a chunk's
    /// 20) and a time at least every 72 hours, ends included, within the
    /// point budget.
    #[test]
    fn preview_thinning_keeps_a_position_in_every_chunk() {
        let t0 = 946_684_800;
        let plan = Plan {
            grid: GridSpec {
                ni: 200,
                nj: 200,
                la1: 50_000_000,
                lo1: 300_000_000,
                step: 250_000,
            },
            times: (0..200).map(|h| t0 + 3600 * h).collect(),
            parts: Parts::ALL,
            interval: Interval::Hourly,
            stokes_drift: false,
        };
        let points = preview_points(&plan);
        assert!(points.len() <= MAX_POINTS, "{}", points.len());
        let mut times: Vec<i64> = points.iter().map(|p| p.t).collect();
        times.sort_unstable();
        times.dedup();
        let hours = widest_gap(&times, t0, t0 + 3600 * 199).unwrap() / 3600;
        assert!(hours <= 72, "{hours} h");
        // Every time keeps the same nodes; their rows and columns as indices.
        let at_first: Vec<&Point> = points.iter().filter(|p| p.t == t0).collect();
        let mut rows: Vec<i64> = at_first
            .iter()
            .map(|p| ((50.0 - p.lat) / 0.25).round() as i64)
            .collect();
        let mut cols: Vec<i64> = at_first
            .iter()
            .map(|p| ((p.lon.rem_euclid(360.0) - 300.0) / 0.25).round() as i64)
            .collect();
        for v in [&mut rows, &mut cols] {
            v.sort_unstable();
            v.dedup();
        }
        assert!(widest_gap(&rows, 0, 199).unwrap() <= 4);
        assert!(widest_gap(&cols, 0, 199).unwrap() <= 4);
        assert_eq!(at_first.len(), rows.len() * cols.len());
    }

    /// A plan within the budget is estimated over every node and time.
    #[test]
    fn a_small_preview_is_not_thinned() {
        let plan = Plan::of(
            &[(0, 50.0, -5.0), (7200, 50.6, -4.2)],
            true,
            true,
            Interval::Hourly,
            false,
        )
        .unwrap();
        assert_eq!(
            preview_points(&plan).len() as u64,
            plan.grid.point_count() * plan.times.len() as u64
        );
    }

    /// Hand-computed: a 20 × 10 grid has two tiles along (16 + 4) and one
    /// down; every node is in exactly one tile.
    #[test]
    fn tiles_cover_every_node_once() {
        let grid = GridSpec {
            ni: 20,
            nj: 10,
            la1: 50_000_000,
            lo1: 0,
            step: 250_000,
        };
        let t = tiles(grid);
        assert_eq!(t.len(), 2);
        assert_eq!((t[0].len(), t[1].len()), (160, 40));
        let mut all: Vec<usize> = t.concat();
        all.sort_unstable();
        assert_eq!(all, (0..200).collect::<Vec<_>>());
        assert_eq!(t[1][0], 16);
    }

    /// Three hours of wind only: 6 messages of 209 + 2 × 374 bytes; with
    /// waves and current, 12 more with a 47-byte bitmap each.
    #[test]
    fn the_file_size_is_the_messages() {
        let fixes = [(3600, 50.0, -5.0), (3 * 3600 - 60, 50.5, -4.5)];
        let wind = Plan::of(&fixes, false, false, Interval::Hourly, false).unwrap();
        assert_eq!((wind.grid.ni, wind.grid.nj), (19, 19));
        assert_eq!(wind.times, vec![3600, 7200, 10800]);
        let nodes = 19 * 19;
        assert_eq!(wind.file_bytes(), 6 * (209 + 2 * nodes));
        let all = Plan::of(&fixes, true, true, Interval::Hourly, false).unwrap();
        assert_eq!(all.messages(), 18);
        assert_eq!(
            all.file_bytes(),
            6 * (209 + 2 * nodes) + 12 * (209 + 2 * nodes + 46 + 1)
        );
        assert_eq!(
            all.parameters(),
            vec![
                Parameter::WindU,
                Parameter::WindV,
                Parameter::WaveHeight,
                Parameter::WaveDirection,
                Parameter::CurrentU,
                Parameter::CurrentV,
            ]
        );
        assert!(Plan::of(&[], true, true, Interval::Hourly, false).is_none());
    }
}
