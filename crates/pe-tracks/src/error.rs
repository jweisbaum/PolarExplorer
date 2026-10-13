//! Why a track file did not import, and where.

use thiserror::Error;

use crate::time::TimeError;

/// The largest track file read, bytes. A season of one-second fixes is a
/// few hundred megabytes; anything larger is not a track of one race.
pub const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;

/// What was wrong. Each has a stable [`Reason::code`] the interface
/// translates.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum Reason {
    /// Nothing in the file.
    #[error("the file is empty")]
    Empty,
    /// Larger than [`MAX_FILE_BYTES`].
    #[error("the file is too large to be a track")]
    TooLarge,
    /// Not text.
    #[error("the file is not text")]
    NotText,
    /// Not JSON at all.
    #[error("this is not JSON: {0}")]
    NotJson(String),
    /// JSON, but not a GeoJSON feature or feature collection.
    #[error("this is not a GeoJSON Feature or FeatureCollection")]
    NotGeoJson,
    /// XML, but not a usable GPX track.
    #[error("this is not a usable GPX track: {0}")]
    BadGpx(String),
    /// A geometry other than a Point, LineString or MultiLineString.
    #[error("a {0} geometry is not a track")]
    UnsupportedGeometry(String),
    /// A position that is not `[lon, lat]` on Earth.
    #[error("this position is not a longitude and latitude on Earth")]
    BadPosition,
    /// A point or vertex with no time.
    #[error("this position has no time")]
    NoTime,
    /// A time that did not read.
    #[error("{0}")]
    BadTime(TimeError),
    /// A line whose times do not match its vertices one to one.
    #[error("the line has {vertices} positions but {times} times")]
    TimesMismatch {
        /// Vertices in the line.
        vertices: usize,
        /// Times given for it.
        times: usize,
    },
    /// A CSV with no header row.
    #[error("the file has no header row")]
    NoHeader,
    /// A CSV row with fewer cells than the mapped columns need.
    #[error("this row has too few cells")]
    ShortRow,
    /// A quoted CSV field that never closes.
    #[error("a quoted field is not closed")]
    UnclosedQuote,
    /// A mapped column that is not in the header.
    #[error("column {0} is not in the file")]
    MissingColumn(usize),
    /// A cell that should be a number.
    #[error("{0:?} is not a number")]
    NotANumber(String),
    /// A heading outside 0–360°, or a negative or absurd speed.
    #[error("{0} is not a heading or speed a boat can have")]
    OutOfRange(f64),
    /// A file (or chosen boat) with no fixes.
    #[error("the file holds no positions")]
    NoFixes,
    /// Too many fixes to keep.
    #[error("the file holds more positions than a track may have")]
    TooManyFixes,
}

impl Reason {
    /// The stable code the frontend translates.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::TooLarge => "too-large",
            Self::NotText => "not-text",
            Self::NotJson(_) => "not-json",
            Self::NotGeoJson => "not-geojson",
            Self::BadGpx(_) => "bad-gpx",
            Self::UnsupportedGeometry(_) => "unsupported-geometry",
            Self::BadPosition => "bad-position",
            Self::NoTime => "no-time",
            Self::BadTime(_) => "bad-time",
            Self::TimesMismatch { .. } => "times-mismatch",
            Self::NoHeader => "no-header",
            Self::ShortRow => "short-row",
            Self::UnclosedQuote => "unclosed-quote",
            Self::MissingColumn(_) => "missing-column",
            Self::NotANumber(_) => "not-a-number",
            Self::OutOfRange(_) => "out-of-range",
            Self::NoFixes => "no-fixes",
            Self::TooManyFixes => "too-many-fixes",
        }
    }
}

/// A refused track file: where, and why. CSV and JSON syntax errors name a
/// line and column; a GeoJSON feature that is well-formed JSON but not a
/// usable track names the feature (0-based, in file order). GPX XML errors
/// name the file-level reason.
#[derive(Debug, Clone, PartialEq, Error)]
#[error("{}{reason}", where_text(*.line, *.column, *.feature))]
pub struct TrackFileError {
    /// 1-based line, when known.
    pub line: Option<usize>,
    /// 1-based column, in characters, when known.
    pub column: Option<usize>,
    /// 0-based GeoJSON feature, when that is the best location there is.
    pub feature: Option<usize>,
    /// What was wrong.
    pub reason: Reason,
}

fn where_text(line: Option<usize>, column: Option<usize>, feature: Option<usize>) -> String {
    match (line, column, feature) {
        (Some(l), Some(c), _) => format!("line {l}, column {c}: "),
        (Some(l), None, _) => format!("line {l}: "),
        (None, _, Some(f)) => format!("feature {f}: "),
        _ => String::new(),
    }
}

impl TrackFileError {
    /// An error about the file as a whole.
    pub fn whole(reason: Reason) -> Self {
        Self {
            line: None,
            column: None,
            feature: None,
            reason,
        }
    }

    /// An error at a line and column.
    pub fn at(line: usize, column: usize, reason: Reason) -> Self {
        Self {
            line: Some(line),
            column: Some(column),
            feature: None,
            reason,
        }
    }

    /// An error in one GeoJSON feature.
    pub fn in_feature(feature: usize, reason: Reason) -> Self {
        Self {
            line: None,
            column: None,
            feature: Some(feature),
            reason,
        }
    }
}

/// Convenience alias.
pub type Result<T> = std::result::Result<T, TrackFileError>;
