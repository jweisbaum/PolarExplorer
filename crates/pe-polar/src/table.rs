//! Adrena and grid polars (`.pol` tab-separated, `.csv` semicolon- or
//! comma-separated, spec.md 6): top-left `TWA\TWS`, `TWA/TWS` or `TWA`,
//! wind speeds across the first row, angles down the first column, boat
//! speeds in the cells. An empty cell, or a row shorter than the header, is
//! an empty cell of the polar. The wind speed row may run to 70 kn
//! ([`crate::MAX_TWS_KN`]); boat speed cells stay capped at 60.

use std::collections::BTreeSet;

use pe_core::polar::PolarGrid;

use crate::build::Builder;
use crate::format::{
    Field, PolarError, Reason, Result, angle, axis_text, bsp_text, fields, is_table_header,
    meaningful, speed, table_separator, tws_speed,
};

/// Drops empty fields at the end of a row: spreadsheets pad rows with
/// separators.
fn trim_trailing(mut row: Vec<Field<'_>>) -> Vec<Field<'_>> {
    while row.len() > 1 && row.last().is_some_and(|f| f.text.is_empty()) {
        row.pop();
    }
    row
}

pub(crate) fn read(text: &str) -> Result<PolarGrid> {
    let mut lines = meaningful(text);
    let Some((header_line, header)) = lines.next() else {
        return Err(PolarError::whole(Reason::Empty));
    };
    let rows: Vec<_> = lines.collect();
    let sep = table_separator(header);
    let head = trim_trailing(fields(header, sep));
    let Some((corner, speeds)) = head.split_first() else {
        return Err(PolarError::whole(Reason::Empty));
    };
    let luckgrib_label = !rows.is_empty()
        && corner.text.parse::<f64>().is_err()
        && !corner.text.eq_ignore_ascii_case("tws");
    if !is_table_header(corner.text) && !luckgrib_label {
        return Err(PolarError::at(
            header_line,
            corner.column,
            Reason::MissingHeader(corner.text.to_owned()),
        ));
    }
    let mut builder = Builder::default();
    let mut seen = BTreeSet::new();
    let mut columns = Vec::with_capacity(speeds.len());
    for field in speeds {
        let tws = tws_speed(header_line, *field, sep)?;
        if !seen.insert(tws.to_bits()) {
            return Err(PolarError::at(
                header_line,
                field.column,
                Reason::DuplicateTws(tws),
            ));
        }
        let key = builder
            .tws(tws)
            .map_err(|reason| PolarError::at(header_line, field.column, reason))?;
        columns.push(key);
    }
    if columns.is_empty() {
        return Err(PolarError::at(header_line, corner.column, Reason::NoSpeeds));
    }

    let mut seen = BTreeSet::new();
    for (line, content) in rows {
        let row = trim_trailing(fields(content, sep));
        let Some((first, cells)) = row.split_first() else {
            continue;
        };
        let raw = angle(line, *first, sep)?;
        if !seen.insert(raw.to_bits()) {
            return Err(PolarError::at(
                line,
                first.column,
                Reason::DuplicateTwa(raw),
            ));
        }
        // Retain both sides so an asymmetric export reads back losslessly.
        let twa = builder
            .twa(raw)
            .map_err(|reason| PolarError::at(line, first.column, reason))?;
        for (index, cell) in cells.iter().enumerate() {
            let Some(tws) = columns.get(index) else {
                return Err(PolarError::at(line, cell.column, Reason::TooManyCells));
            };
            if cell.text.is_empty() {
                continue;
            }
            builder.cell(twa, *tws, speed(line, *cell, sep)?);
        }
    }
    if !builder.has_cells() {
        return Err(PolarError::at(header_line, corner.column, Reason::NoSpeeds));
    }
    Ok(builder.finish())
}

/// The whole grid, empty cells left empty.
pub(crate) fn write(polar: &PolarGrid, separator: char, corner: &str) -> String {
    let sep = separator.to_string();
    let mut out = String::from(corner);
    for tws in &polar.tws {
        out.push_str(&sep);
        out.push_str(&axis_text(*tws));
    }
    out.push('\n');
    for (i, twa) in polar.twa.iter().enumerate() {
        out.push_str(&axis_text(*twa));
        for j in 0..polar.tws.len() {
            out.push_str(&sep);
            if let Some(bsp) = polar.get(i, j) {
                out.push_str(&bsp_text(bsp));
            }
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(text: &str) -> PolarError {
        read(text).unwrap_err()
    }

    #[test]
    fn every_header_variant_and_separator_reads_the_same() {
        let expected = PolarGrid {
            twa: vec![40.0, 90.0],
            tws: vec![6.0, 8.0],
            bsp: vec![vec![Some(5.0), Some(5.5)], vec![Some(6.25), None]],
        };
        for text in [
            "TWA\\TWS\t6\t8\n40\t5\t5.5\n90\t6.25\n",
            "TWA/TWS;6;8;\n40;5;5,5;\n90;6,25;;\n",
            "twa,6,8\n90,6.25,\n40,5,5.5\n",
            "\"TWA\",\"6\",\"8\"\r\n\"40\",\"5\",\"5.5\"\r\n\"90\",\"6.25\"\r\n",
            "TWA\\TWS 6 8\n40 5 5.5\n90 6.25\n",
        ] {
            assert_eq!(read(text).unwrap(), expected, "{text:?}");
        }
    }

    #[test]
    fn port_rows_keep_independent_values() {
        let polar = read("TWA\t6\n40\t5\n320\t6\n").unwrap();
        assert_eq!(polar.twa, [40.0, 320.0]);
        assert_eq!(polar.bsp, [vec![Some(5.0)], vec![Some(6.0)]]);
    }

    #[test]
    fn errors_name_line_and_column() {
        let e = err("TWS\t6\n");
        assert_eq!((e.line, e.column), (1, 1));
        assert_eq!(e.reason, Reason::MissingHeader("TWS".into()));

        let e = err("TWA;6;x\n");
        assert_eq!((e.column, e.reason), (7, Reason::NotANumber("x".into())));
        let e = err("TWA;6;6\n");
        assert_eq!((e.column, e.reason), (7, Reason::DuplicateTws(6.0)));
        let e = err("TWA;6\n40;5;7\n");
        assert_eq!((e.line, e.column, e.reason), (2, 6, Reason::TooManyCells));
        let e = err("TWA;6\n40;5\n40;6\n");
        assert_eq!((e.line, e.reason), (3, Reason::DuplicateTwa(40.0)));
        let e = err("TWA;6\n40;65\n");
        assert_eq!((e.column, e.reason), (4, Reason::TooFast(65.0)));
        // The TWS header may run to 70 kn; 71 is refused.
        assert_eq!(read("TWA;70\n40;5\n").unwrap().tws, [70.0]);
        let e = err("TWA;71\n40;5\n");
        assert_eq!((e.column, e.reason), (5, Reason::TooFast(71.0)));
        let e = err("TWA;6\n-40;5\n");
        assert_eq!((e.column, e.reason), (1, Reason::Negative(-40.0)));
        assert_eq!(err("TWA;6\n40;\n").reason, Reason::NoSpeeds);
        assert_eq!(err("TWA\n40\n").reason, Reason::NoSpeeds);
        // With commas as the separator, a decimal comma splits a cell.
        let e = err("TWA,6\n40,5,5\n");
        assert_eq!(e.reason, Reason::TooManyCells);
    }

    #[test]
    fn writing_is_the_whole_grid() {
        let polar = read("TWA\t6\t8\n40\t5\t5.5\n90\t6.25\n").unwrap();
        assert_eq!(
            write(&polar, '\t', "TWA\\TWS"),
            "TWA\\TWS\t6\t8\n40\t5.00\t5.50\n90\t6.25\t\n"
        );
        assert_eq!(
            write(&polar, ';', "TWA\\TWS"),
            "TWA\\TWS;6;8\n40;5.00;5.50\n90;6.25;\n"
        );
        assert_eq!(read(&write(&polar, ';', "TWA\\TWS")).unwrap(), polar);
    }
}
