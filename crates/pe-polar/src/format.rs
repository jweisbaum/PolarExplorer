//! Polar files: telling the formats apart, reading them, writing them
//! (spec.md 6).
//!
//! Two layouts are read. **Expedition** (`.txt`) has one row per wind speed:
//! `TWS` then `TWA BSP` pairs, and rows may have different lengths, so the
//! polar keeps each wind speed's own angles on a union TWA axis with empty
//! cells where a row has no point. **Adrena / grid** (`.pol` tab-separated,
//! `.csv` semicolon- or comma-separated), **qtVlm** (`.csv`, semicolon),
//! **OpenCPN** (`.pol`, semicolon) and **LuckGrib** (spreadsheet CSV/TSV) are
//! tables with wind speeds across and angles down.
//!
//! The format is decided from the content, never from the extension alone: a
//! first meaningful line starting with a recognized table header is a table, one
//! starting with a number is Expedition.
//!
//! **Every error names its line and column** (1-based, counted in
//! characters), and nothing a file holds can make the reader panic: it
//! returns an error instead (tested with arbitrary bytes). Angles past 180°
//! are folded (`360 − TWA`); where a file gives both sides of one angle, the
//! two are averaged.
//!
//! The writers are byte-deterministic because export uses them (invariant 5):
//! axes with at most two decimals and no trailing zeros, boat speeds with
//! exactly two, `\n` line endings, and nothing that depends on the machine.
//! Their output is pinned by the golden files in `tests/golden/`.

use pe_core::polar::{PolarFileFormat, PolarGrid};
use thiserror::Error;

use crate::{MAX_SPEED_KN, MAX_TWS_KN, expedition, table};

/// The largest polar file read. Text formats remain small; the larger limit
/// accommodates compressed WhirlwindMatrixPolar matrices.
pub const MAX_FILE_BYTES: usize = crate::matrix::MAX_MATRIX_FILE_BYTES;

/// The most distinct values either axis may have. Real polars have tens; the
/// limit bounds the grid a hostile file could make the reader allocate (an
/// Expedition file's union axis multiplies rows by angles).
pub const MAX_AXIS_VALUES: usize = 512;

/// Why a polar file was refused. The stable [`Reason::code`] is what the
/// interface translates; the `Display` text is English, for logs.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum Reason {
    /// The file holds nothing but blank or comment lines.
    #[error("the file is empty")]
    Empty,
    /// Larger than [`MAX_FILE_BYTES`].
    #[error("the file is too large to be a polar")]
    TooLarge,
    /// Neither a `TWA` header nor a leading number.
    #[error("this is neither an Expedition polar nor a TWA × TWS table")]
    UnknownFormat,
    /// A table whose top-left cell is not `TWA\TWS`, `TWA/TWS` or `TWA`.
    #[error("the top-left cell must be TWA\\TWS, TWA/TWS or TWA, not {0:?}")]
    MissingHeader(String),
    /// A field that should be a number is not one.
    #[error("{0:?} is not a number")]
    NotANumber(String),
    /// A negative speed or angle.
    #[error("{0} is negative")]
    Negative(f64),
    /// A boat speed above [`MAX_SPEED_KN`], or a wind speed above
    /// [`MAX_TWS_KN`].
    #[error("{0} kn is faster than a polar holds")]
    TooFast(f64),
    /// An angle past 360°.
    #[error("{0}° is not a wind angle (0–360)")]
    AngleOutOfRange(f64),
    /// The same wind speed twice.
    #[error("the wind speed {0} kn appears twice")]
    DuplicateTws(f64),
    /// The same angle twice for one wind speed, or two table rows.
    #[error("the wind angle {0}° appears twice")]
    DuplicateTwa(f64),
    /// An Expedition row ending in an angle with no boat speed.
    #[error("this wind angle has no boat speed after it")]
    MissingBsp,
    /// A table row with more cells than the header has wind speeds.
    #[error("this row has more cells than the header has wind speeds")]
    TooManyCells,
    /// More than [`MAX_AXIS_VALUES`] angles or wind speeds.
    #[error("the polar has more than {MAX_AXIS_VALUES} wind angles or wind speeds")]
    TooManyValues,
    /// A readable file with no boat speed in it.
    #[error("the file holds no boat speeds")]
    NoSpeeds,
    /// A malformed WhirlwindMatrixPolar binary file.
    #[error("the WhirlwindMatrixPolar data is invalid: {0}")]
    MatrixInvalid(String),
}

impl Reason {
    /// Every [`Reason::code`], for the interface's translation table to be
    /// checked against.
    pub const CODES: [&'static str; 15] = [
        "empty",
        "too-large",
        "unknown-format",
        "missing-header",
        "not-a-number",
        "negative",
        "too-fast",
        "angle-out-of-range",
        "duplicate-tws",
        "duplicate-twa",
        "missing-bsp",
        "too-many-cells",
        "too-many-values",
        "no-speeds",
        "matrix-invalid",
    ];

    /// A stable identifier for the interface to translate by.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::TooLarge => "too-large",
            Self::UnknownFormat => "unknown-format",
            Self::MissingHeader(_) => "missing-header",
            Self::NotANumber(_) => "not-a-number",
            Self::Negative(_) => "negative",
            Self::TooFast(_) => "too-fast",
            Self::AngleOutOfRange(_) => "angle-out-of-range",
            Self::DuplicateTws(_) => "duplicate-tws",
            Self::DuplicateTwa(_) => "duplicate-twa",
            Self::MissingBsp => "missing-bsp",
            Self::TooManyCells => "too-many-cells",
            Self::TooManyValues => "too-many-values",
            Self::NoSpeeds => "no-speeds",
            Self::MatrixInvalid(_) => "matrix-invalid",
        }
    }
}

/// A refused polar file: where, and why.
#[derive(Debug, Clone, PartialEq, Error)]
#[error("line {line}, column {column}: {reason}")]
pub struct PolarError {
    /// 1-based line.
    pub line: usize,
    /// 1-based column, in characters.
    pub column: usize,
    /// What was wrong there.
    pub reason: Reason,
}

impl PolarError {
    pub(crate) fn at(line: usize, column: usize, reason: Reason) -> Self {
        Self {
            line,
            column,
            reason,
        }
    }

    /// An error about the file as a whole, reported at its start.
    pub(crate) fn whole(reason: Reason) -> Self {
        Self::at(1, 1, reason)
    }
}

pub(crate) type Result<T> = std::result::Result<T, PolarError>;

/// A polar as read from a file.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    /// The format it was read as.
    pub format: PolarFileFormat,
    /// The grid.
    pub polar: PolarGrid,
    /// Full matrix payload, retained when importing WhirlwindMatrixPolar.
    pub matrix: Option<pe_core::polar::WhirlwindMatrixPolar>,
}

/// Decodes a file's bytes: UTF-8 (with or without a BOM), UTF-16 with a BOM
/// (what spreadsheets call "Unicode text"), and otherwise Latin-1, which
/// decodes every byte. Line endings become `\n`.
pub(crate) fn decode(bytes: &[u8]) -> String {
    let text = if let Some(rest) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        String::from_utf8_lossy(rest).into_owned()
    } else if let Some(rest) = bytes.strip_prefix(b"\xFF\xFE") {
        utf16(rest, u16::from_le_bytes)
    } else if let Some(rest) = bytes.strip_prefix(b"\xFE\xFF") {
        utf16(rest, u16::from_be_bytes)
    } else {
        match std::str::from_utf8(bytes) {
            Ok(text) => text.to_owned(),
            Err(_) => bytes.iter().map(|byte| char::from(*byte)).collect(),
        }
    };
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn utf16(bytes: &[u8], word: fn([u8; 2]) -> u16) -> String {
    let words = bytes.chunks_exact(2).map(|pair| word([pair[0], pair[1]]));
    char::decode_utf16(words)
        .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

/// A line worth reading: not blank, not an `!` comment.
pub(crate) fn meaningful(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.split('\n').enumerate().filter_map(|(i, line)| {
        let trimmed = line.trim();
        (!trimmed.is_empty() && !trimmed.starts_with('!')).then_some((i + 1, line))
    })
}

/// How a table's cells are separated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Separator {
    Tab,
    Semicolon,
    Comma,
    /// Runs of spaces (or tabs), as Expedition rows are.
    Whitespace,
}

impl Separator {
    /// Whether a comma inside a number is its decimal point. Only where the
    /// comma cannot be the separator: French and German spreadsheets write
    /// `5,25` in semicolon files.
    pub(crate) fn decimal_comma(self) -> bool {
        self != Self::Comma
    }
}

/// One field of a line and the column it starts at.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Field<'a> {
    pub text: &'a str,
    pub column: usize,
}

/// Splits a line into fields, trimmed and unquoted, each with its 1-based
/// character column. With a character separator, empty fields are kept
/// (`a;;b` has three); with whitespace, runs of it are one separator.
pub(crate) fn fields(line: &str, separator: Separator) -> Vec<Field<'_>> {
    let is_sep = |c: char| match separator {
        Separator::Tab => c == '\t',
        Separator::Semicolon => c == ';',
        Separator::Comma => c == ',',
        Separator::Whitespace => c.is_whitespace(),
    };
    let whitespace = separator == Separator::Whitespace;
    let mut out = Vec::new();
    // (byte offset, column) where the current field starts.
    let mut start: Option<(usize, usize)> = (!whitespace).then_some((0, 1));
    for (index, (byte, c)) in line.char_indices().enumerate() {
        let column = index + 1;
        if is_sep(c) {
            if let Some((from, col)) = start.take() {
                out.push(field(&line[from..byte], col));
            }
            if !whitespace {
                start = Some((byte + c.len_utf8(), column + 1));
            }
        } else if start.is_none() {
            start = Some((byte, column));
        }
    }
    if let Some((from, col)) = start {
        out.push(field(&line[from..], col));
    }
    out
}

fn field(raw: &str, column: usize) -> Field<'_> {
    let leading = raw.chars().take_while(|c| c.is_whitespace()).count();
    let mut text = raw.trim();
    if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
        text = text[1..text.len() - 1].trim();
    }
    Field {
        text,
        column: column + leading,
    }
}

/// A number in a field. Non-finite values (`inf`, `NaN`) are not numbers here.
pub(crate) fn number(line: usize, field: Field<'_>, separator: Separator) -> Result<f64> {
    let text = if separator.decimal_comma() {
        field.text.replace(',', ".")
    } else {
        field.text.to_owned()
    };
    match text.parse::<f64>() {
        Ok(value) if value.is_finite() => Ok(value),
        _ => Err(PolarError::at(
            line,
            field.column,
            Reason::NotANumber(field.text.to_owned()),
        )),
    }
}

/// A speed field: non-negative, at most `max` (knots).
fn bounded_speed(line: usize, field: Field<'_>, separator: Separator, max: f64) -> Result<f64> {
    let value = number(line, field, separator)?;
    if value < 0.0 {
        Err(PolarError::at(line, field.column, Reason::Negative(value)))
    } else if value > max {
        Err(PolarError::at(line, field.column, Reason::TooFast(value)))
    } else {
        Ok(value)
    }
}

/// A boat speed (BSP), knots: 0 to [`MAX_SPEED_KN`].
pub(crate) fn speed(line: usize, field: Field<'_>, separator: Separator) -> Result<f64> {
    bounded_speed(line, field, separator, MAX_SPEED_KN)
}

/// A wind speed (TWS), knots: 0 to [`MAX_TWS_KN`] — higher than a boat speed,
/// because real polars carry a TWS axis into gale-force wind (spec.md 6).
pub(crate) fn tws_speed(line: usize, field: Field<'_>, separator: Separator) -> Result<f64> {
    bounded_speed(line, field, separator, MAX_TWS_KN)
}

/// The words an Expedition label/header row is made of — a row naming its
/// columns instead of holding the first wind speed's data (spec.md 6), e.g.
/// `twa0 bsp0 TwaUp bspUp` or `pol Twa0 Bsp0 UpTwa UpBsp`. Ordered longest
/// first so `upwind` is not read as `up` + `wind`.
const LABEL_WORDS: [&str; 8] = ["downwind", "upwind", "twa", "bsp", "tws", "pol", "up", "dn"];

/// Whether a cell is one of [`LABEL_WORDS`], possibly joined (`TwaUp`,
/// `UpBsp`) and/or followed by digits (`twa0`, `Bsp1`), case-insensitively.
/// A cell that is only digits (a real TWS value) is not a label.
fn is_label_cell(cell: &str) -> bool {
    let letters: String = cell
        .chars()
        .filter(char::is_ascii_alphabetic)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let mut rest = letters.as_str();
    if rest.is_empty() {
        return false;
    }
    while !rest.is_empty() {
        match LABEL_WORDS.iter().find_map(|word| rest.strip_prefix(word)) {
            Some(after) => rest = after,
            None => return false,
        }
    }
    true
}

/// Whether a row is a leading Expedition label row: every cell a
/// [`is_label_cell`] word, and at least one cell. Such a row is skipped, not
/// read as a wind speed with no points (spec.md 6).
pub(crate) fn is_expedition_label_row(row: &[Field<'_>]) -> bool {
    !row.is_empty() && row.iter().all(|f| is_label_cell(f.text))
}

/// A true wind angle, degrees 0–360, as written (not yet folded).
pub(crate) fn angle(line: usize, field: Field<'_>, separator: Separator) -> Result<f64> {
    let value = number(line, field, separator)?;
    if value < 0.0 {
        Err(PolarError::at(line, field.column, Reason::Negative(value)))
    } else if value > 360.0 {
        Err(PolarError::at(
            line,
            field.column,
            Reason::AngleOutOfRange(value),
        ))
    } else {
        Ok(value)
    }
}

/// Whether a cell is the top-left header of a table.
pub(crate) fn is_table_header(cell: &str) -> bool {
    let cell = cell.to_ascii_lowercase();
    matches!(cell.as_str(), "twa\\tws" | "twa/tws" | "twa")
}

/// The separator of a table, from its header line.
pub(crate) fn table_separator(header: &str) -> Separator {
    if header.contains('\t') {
        Separator::Tab
    } else if header.contains(';') {
        Separator::Semicolon
    } else if header.contains(',') {
        Separator::Comma
    } else {
        Separator::Whitespace
    }
}

/// Tells the format from the content (spec.md 6).
pub fn detect(bytes: &[u8]) -> std::result::Result<PolarFileFormat, PolarError> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(PolarError::whole(Reason::TooLarge));
    }
    if bytes.starts_with(b"WWMPOLAR") {
        return Ok(PolarFileFormat::WhirlwindMatrixPolar);
    }
    detect_text(&decode(bytes))
}

fn detect_text(text: &str) -> Result<PolarFileFormat> {
    let mut lines = meaningful(text).peekable();
    let Some((line, first)) = lines.next() else {
        return Err(PolarError::whole(Reason::Empty));
    };
    let separator = table_separator(first);
    let lead = fields(first, separator);
    // LuckGrib accepts spreadsheet tables whose top-left label is ignored.
    // Recognize that shape by its nonnumeric corner, numeric wind-speed
    // row and angle rows. Expedition starts with a number too, but that
    // number is a wind speed, not a table corner.
    if lead.len() >= 2
        && !lead.first().is_some_and(|field| {
            is_table_header(field.text) || number(line, *field, separator).is_ok()
        })
        && lead[1..]
            .iter()
            .all(|field| tws_speed(line, *field, separator).is_ok())
        && lines.peek().is_some_and(|(row_line, row)| {
            fields(row, separator)
                .first()
                .is_some_and(|field| angle(*row_line, *field, separator).is_ok())
        })
    {
        return Ok(PolarFileFormat::LuckGrib);
    }
    if lead.first().is_some_and(|f| is_table_header(f.text)) {
        return Ok(match separator {
            // qtVlm and OpenCPN use the same headers as existing CSV
            // polars. Content alone cannot identify their producer; keep
            // the canonical CSV classification and its round-trip bytes.
            Separator::Tab | Separator::Whitespace
                if lead
                    .first()
                    .is_some_and(|field| field.text.eq_ignore_ascii_case("TWA")) =>
            {
                PolarFileFormat::LuckGrib
            }
            Separator::Semicolon | Separator::Comma => PolarFileFormat::Csv,
            Separator::Tab | Separator::Whitespace => PolarFileFormat::Adrena,
        });
    }
    let starts_with_number = |text: &str| {
        fields(text, Separator::Whitespace)
            .first()
            .is_some_and(|f| number(line, *f, Separator::Whitespace).is_ok())
    };
    if starts_with_number(first) {
        return Ok(PolarFileFormat::Expedition);
    }
    // A leading label row (`twa0 bsp0 TwaUp bspUp …`) is not itself a wind
    // speed; it is Expedition only if the row after it is one.
    if is_expedition_label_row(&fields(first, Separator::Whitespace))
        && lines
            .next()
            .is_some_and(|(_, next)| starts_with_number(next))
    {
        return Ok(PolarFileFormat::Expedition);
    }
    let column = first.chars().take_while(|c| c.is_whitespace()).count() + 1;
    Err(PolarError::at(line, column, Reason::UnknownFormat))
}

/// Reads a polar file of any supported format.
pub fn read(bytes: &[u8]) -> std::result::Result<Parsed, PolarError> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(PolarError::whole(Reason::TooLarge));
    }
    if bytes.starts_with(b"WWMPOLAR") {
        let matrix = crate::matrix::read(bytes)
            .map_err(|error| PolarError::whole(Reason::MatrixInvalid(error.to_string())))?;
        let polar = crate::matrix::MatrixPolarExt::to_polar(&matrix)
            .map_err(|error| PolarError::whole(Reason::MatrixInvalid(error.to_string())))?;
        return Ok(Parsed {
            format: PolarFileFormat::WhirlwindMatrixPolar,
            polar,
            matrix: Some(matrix),
        });
    }
    let text = decode(bytes);
    let format = detect_text(&text)?;
    parse(format, &text)
}

/// Reads a polar file as the given format.
pub fn read_as(
    format: PolarFileFormat,
    bytes: &[u8],
) -> std::result::Result<PolarGrid, PolarError> {
    if format == PolarFileFormat::WhirlwindMatrixPolar {
        return read(bytes).map(|parsed| parsed.polar);
    }
    if bytes.len() > MAX_FILE_BYTES {
        return Err(PolarError::whole(Reason::TooLarge));
    }
    Ok(parse(format, &decode(bytes))?.polar)
}

fn parse(format: PolarFileFormat, text: &str) -> Result<Parsed> {
    let polar = match format {
        PolarFileFormat::Expedition => expedition::read(text)?,
        PolarFileFormat::Adrena
        | PolarFileFormat::Csv
        | PolarFileFormat::QtVlm
        | PolarFileFormat::OpenCpn
        | PolarFileFormat::LuckGrib => table::read(text)?,
        PolarFileFormat::WhirlwindMatrixPolar => {
            return Err(PolarError::whole(Reason::UnknownFormat));
        }
    };
    Ok(Parsed {
        format,
        polar,
        matrix: None,
    })
}

/// Writes a polar in the given format. Deterministic to the byte.
pub fn write(format: PolarFileFormat, polar: &PolarGrid) -> String {
    match format {
        PolarFileFormat::Expedition => expedition::write(polar),
        PolarFileFormat::Adrena => table::write(polar, '\t', "TWA\\TWS"),
        PolarFileFormat::Csv => table::write(polar, ';', "TWA\\TWS"),
        PolarFileFormat::QtVlm => table::write(polar, ';', "TWA\\TWS"),
        PolarFileFormat::OpenCpn => table::write(polar, ';', "twa/tws"),
        PolarFileFormat::LuckGrib => table::write(polar, '\t', "TWA"),
        // Binary output is provided by `matrix::write`.
        PolarFileFormat::WhirlwindMatrixPolar => String::new(),
    }
}

/// An axis value: at most two decimals, no trailing zeros (`52`, `42.5`).
pub(crate) fn axis_text(value: f64) -> String {
    let text = format!("{:.2}", non_negative_zero(value));
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}

/// A boat speed: exactly two decimals (`5.20`).
pub(crate) fn bsp_text(value: f64) -> String {
    let text = format!("{:.2}", non_negative_zero(value));
    if text == "-0.00" {
        "0.00".to_owned()
    } else {
        text
    }
}

fn non_negative_zero(value: f64) -> f64 {
    if value == 0.0 { 0.0 } else { value }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(line: &str, separator: Separator) -> Vec<(String, usize)> {
        fields(line, separator)
            .into_iter()
            .map(|f| (f.text.to_owned(), f.column))
            .collect()
    }

    #[test]
    fn fields_carry_their_columns() {
        assert_eq!(
            texts("6  42.5\t5.1", Separator::Whitespace),
            [("6".into(), 1), ("42.5".into(), 4), ("5.1".into(), 9)]
        );
        assert_eq!(
            texts("TWA;6; 8;", Separator::Semicolon),
            [
                ("TWA".into(), 1),
                ("6".into(), 5),
                ("8".into(), 8),
                ("".into(), 10)
            ]
        );
        assert_eq!(
            texts("\"TWA\\TWS\",\"6\"", Separator::Comma),
            [("TWA\\TWS".into(), 1), ("6".into(), 11)]
        );
        assert_eq!(
            texts("é\tx", Separator::Tab),
            [("é".into(), 1), ("x".into(), 3)]
        );
    }

    #[test]
    fn formats_are_told_apart_by_content() {
        let d = |t: &str| detect(t.as_bytes());
        assert_eq!(d("!c\n6 40 5\n"), Ok(PolarFileFormat::Expedition));
        assert_eq!(d("TWA\\TWS\t6\n40\t5\n"), Ok(PolarFileFormat::Adrena));
        assert_eq!(d("twa/tws;6\n40;5\n"), Ok(PolarFileFormat::Csv));
        assert_eq!(d("TWA,6\n40,5\n"), Ok(PolarFileFormat::Csv));
        assert_eq!(d("TWA\\TWS 6 8\n"), Ok(PolarFileFormat::Adrena));
        assert_eq!(d("\n\n!only\n").unwrap_err().reason, Reason::Empty);
        let unknown = d("\n  hello 6 7\n").unwrap_err();
        assert_eq!((unknown.line, unknown.column), (2, 3));
        assert_eq!(unknown.reason, Reason::UnknownFormat);
    }

    #[test]
    fn numeric_expedition_rows_are_not_spreadsheet_headers() {
        let parsed = read(b"6 40 5\n8 40 5.2\n").unwrap();
        assert_eq!(parsed.format, PolarFileFormat::Expedition);
        assert_eq!(parsed.polar.twa, [40.0]);
        assert_eq!(parsed.polar.tws, [6.0, 8.0]);
        assert_eq!(parsed.polar.bsp, [vec![Some(5.0), Some(5.2)]]);
    }

    #[test]
    fn spreadsheet_variants_preserve_values_and_classify_shared_headers_as_csv() {
        for (text, format) in [
            ("TWA\\TWS;6;8\n40;5;5.5\n", PolarFileFormat::Csv),
            ("twa/tws;6;8\n40;5;5.5\n", PolarFileFormat::Csv),
            ("Boat\t6\t8\n40\t5\t5.5\n", PolarFileFormat::LuckGrib),
        ] {
            let parsed = read(text.as_bytes()).unwrap();
            assert_eq!(parsed.format, format);
            assert_eq!(parsed.polar.twa, [40.0]);
            assert_eq!(parsed.polar.tws, [6.0, 8.0]);
            assert_eq!(parsed.polar.bsp, [vec![Some(5.0), Some(5.5)]]);
        }
    }

    #[test]
    fn an_expedition_label_row_is_only_a_label_before_a_wind_speed() {
        let d = |t: &str| detect(t.as_bytes());
        // Swan 78.txt and J35.txt shaped label rows, tab- and
        // space-separated.
        assert_eq!(
            d("\ttwa0\tbsp0\tTwaUp\tbspUp\n6\t0\t0\t45\t7.04\n"),
            Ok(PolarFileFormat::Expedition)
        );
        assert_eq!(
            d("pol  Twa0  Bsp0  UpTwa  UpBsp\n6.3  30  0  45.1  4.95\n"),
            Ok(PolarFileFormat::Expedition)
        );
        // A word row not followed by a numeric row is not Expedition.
        let unknown = d("twa0 bsp0\nhello world\n").unwrap_err();
        assert_eq!(unknown.reason, Reason::UnknownFormat);
        // A lone label row (nothing after it) is not Expedition either.
        assert_eq!(
            d("pol twa bsp\n").unwrap_err().reason,
            Reason::UnknownFormat
        );
    }

    #[test]
    fn label_cells_need_a_word_not_just_digits() {
        for word in ["twa0", "Bsp0", "TwaUp", "UpBsp", "downwind", "pol", "DnTwa"] {
            assert!(is_label_cell(word), "{word:?} should be a label cell");
        }
        for not_word in ["6", "40.5", "0", "-1", ""] {
            assert!(!is_label_cell(not_word), "{not_word:?} should not be");
        }
    }

    #[test]
    fn text_is_decoded_whatever_the_encoding() {
        assert_eq!(decode(b"\xEF\xBB\xBFa\r\nb\rc"), "a\nb\nc");
        assert_eq!(decode(b"\xFF\xFEa\0b\0"), "ab");
        assert_eq!(decode(b"\xFE\xFF\0a\0b"), "ab");
        assert_eq!(decode(b"caf\xE9"), "café");
    }

    /// `CODES` lists every variant's code once. The match has no wildcard, so
    /// a new variant does not compile until it is named here.
    #[test]
    fn every_reason_code_is_listed() {
        let every = [
            Reason::Empty,
            Reason::TooLarge,
            Reason::UnknownFormat,
            Reason::MissingHeader(String::new()),
            Reason::NotANumber(String::new()),
            Reason::Negative(-1.0),
            Reason::TooFast(61.0),
            Reason::AngleOutOfRange(400.0),
            Reason::DuplicateTws(6.0),
            Reason::DuplicateTwa(40.0),
            Reason::MissingBsp,
            Reason::TooManyCells,
            Reason::TooManyValues,
            Reason::NoSpeeds,
            Reason::MatrixInvalid(String::new()),
        ];
        for reason in &every {
            match reason {
                Reason::Empty
                | Reason::TooLarge
                | Reason::UnknownFormat
                | Reason::MissingHeader(_)
                | Reason::NotANumber(_)
                | Reason::Negative(_)
                | Reason::TooFast(_)
                | Reason::AngleOutOfRange(_)
                | Reason::DuplicateTws(_)
                | Reason::DuplicateTwa(_)
                | Reason::MissingBsp
                | Reason::TooManyCells
                | Reason::TooManyValues
                | Reason::NoSpeeds
                | Reason::MatrixInvalid(_) => {}
            }
        }
        let codes: Vec<&str> = every.iter().map(Reason::code).collect();
        assert_eq!(codes, Reason::CODES);
    }

    #[test]
    fn numbers_are_formatted_the_same_everywhere() {
        assert_eq!(axis_text(52.0), "52");
        assert_eq!(axis_text(42.5), "42.5");
        assert_eq!(axis_text(42.25), "42.25");
        assert_eq!(axis_text(0.0), "0");
        assert_eq!(axis_text(-0.0), "0");
        assert_eq!(bsp_text(5.2), "5.20");
        assert_eq!(bsp_text(-0.0), "0.00");
        assert_eq!(bsp_text(12.345_678), "12.35");
    }
}
