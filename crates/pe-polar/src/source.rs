//! A source's own polar, as the views draw it and as the blend will read it.
//!
//! An ORC certificate's grid is its VPP ([`crate::vpp_to_polar`]); a polar
//! file's is the grid it was parsed into. A track has no grid of its own —
//! its polar segment is binned from samples (spec.md 12.1, M13).
//!
//! Three readings, because the overlay means different things to each: the
//! source as imported ([`source_polar`]); the source as edited
//! ([`edited_polar`], what the 3D view and the table draw, with excluded
//! nodes marked rather than hidden — spec.md 10.3: drawn as crosses); and
//! what the blend reads ([`blend_input`]: edited, with every excluded node's
//! cell empty — spec.md 10.3, 12.3). A track's segment goes through the same
//! overlay with [`with_overlay`].

use pe_core::source::{Overlay, Source, SourceKind};

use crate::Polar;

/// The source's grid as imported, or `None` for a track.
pub fn source_polar(source: &Source) -> Option<Polar> {
    match &source.kind {
        SourceKind::Orc { record } => Some(crate::vpp_to_polar(&record.vpp)),
        SourceKind::Orr { record } => Some(record.polar.clone()),
        SourceKind::PolarFile { polar, .. } => Some(polar.clone()),
        SourceKind::Track { .. } => None,
    }
}

/// The source's grid with its edits written in (spec.md 10.4), or `None`
/// for a track. Without overrides it is [`source_polar`] exactly.
pub fn edited_polar(source: &Source) -> Option<Polar> {
    Some(with_overlay(source_polar(source)?, &source.overlay, false))
}

/// The grid the blend reads for this source: its own with its edits, and
/// every excluded node's cell empty. `None` for a track. Removing every
/// override and exclusion gives [`source_polar`] back exactly (invariant 1).
pub fn blend_input(source: &Source) -> Option<Polar> {
    Some(with_overlay(source_polar(source)?, &source.overlay, true))
}

/// A grid read through an overlay: its cell overrides written in (spec.md
/// 12.3: "cell overrides apply before blending"), and, when `exclude`, its
/// excluded nodes emptied. A track's segment goes through this too.
pub fn with_overlay(mut polar: Polar, overlay: &Overlay, exclude: bool) -> Polar {
    crate::edit::apply_overrides(&mut polar, &overlay.cell_overrides);
    if !exclude || overlay.excluded_cells.is_empty() {
        return polar;
    }
    for (i, twa) in polar.twa.iter().enumerate() {
        for (j, tws) in polar.tws.iter().enumerate() {
            if overlay.is_cell_excluded(*twa, *tws)
                && let Some(cell) = polar.bsp.get_mut(i).and_then(|row| row.get_mut(j))
            {
                *cell = None;
            }
        }
    }
    polar
}

#[cfg(test)]
mod tests {
    use pe_core::polar::{PolarFileFormat, PolarGrid};
    use pe_core::source::CellRef;
    use pe_core::{Colour, SourceId};

    use super::*;

    fn file_source() -> Source {
        Source::new(
            SourceId(1),
            "a.txt",
            Colour::parse("#4e79a7").unwrap(),
            SourceKind::PolarFile {
                format: PolarFileFormat::Expedition,
                file_name: "a.txt".to_owned(),
                polar: PolarGrid {
                    twa: vec![45.0, 90.0],
                    tws: vec![6.0, 12.0],
                    bsp: vec![vec![Some(5.5), Some(7.0)], vec![Some(6.5), Some(8.0)]],
                },
                matrix: None,
            },
        )
    }

    #[test]
    fn an_excluded_node_is_empty_for_the_blend_but_not_for_the_views() {
        let mut source = file_source();
        source.overlay.excluded_cells = vec![CellRef {
            twa: 90.0,
            tws: 6.0,
        }];
        let blend = blend_input(&source).unwrap();
        assert_eq!(
            blend.bsp,
            vec![vec![Some(5.5), Some(7.0)], vec![None, Some(8.0)]]
        );
        let drawn = source_polar(&source).unwrap();
        assert_eq!(drawn.bsp[1][0], Some(6.5));
    }

    /// An edit shows in the views and the blend; an excluded edited node is
    /// still empty for the blend; without either, all three readings are the
    /// source exactly.
    #[test]
    fn edits_are_read_by_the_views_and_the_blend() {
        let mut source = file_source();
        source.overlay.cell_overrides = vec![
            pe_core::source::CellOverride {
                twa: 45.0,
                tws: 12.0,
                bsp: 7.4,
            },
            pe_core::source::CellOverride {
                twa: 90.0,
                tws: 6.0,
                bsp: 6.0,
            },
        ];
        source.overlay.excluded_cells = vec![CellRef {
            twa: 90.0,
            tws: 6.0,
        }];
        let edited = edited_polar(&source).unwrap();
        assert_eq!(
            edited.bsp,
            vec![vec![Some(5.5), Some(7.4)], vec![Some(6.0), Some(8.0)]]
        );
        let blend = blend_input(&source).unwrap();
        assert_eq!(
            blend.bsp,
            vec![vec![Some(5.5), Some(7.4)], vec![None, Some(8.0)]]
        );
        assert_eq!(source_polar(&source).unwrap().bsp[0][1], Some(7.0));
        source.overlay = Default::default();
        assert_eq!(edited_polar(&source), source_polar(&source));
    }

    #[test]
    fn without_exclusions_the_blend_reads_the_source_exactly() {
        let source = file_source();
        assert_eq!(blend_input(&source), source_polar(&source));
    }

    #[test]
    fn a_track_has_no_grid() {
        let origin = pe_core::track::TrackOrigin::File {
            name: "race.csv".to_owned(),
            boat_name: None,
        };
        let track = Source::new(
            SourceId(2),
            "Track",
            Colour::parse("#e15759").unwrap(),
            SourceKind::Track {
                track: Box::new(pe_core::track::Track::new(pe_core::TrackId(3), origin)),
            },
        );
        assert_eq!(source_polar(&track), None);
        assert_eq!(blend_input(&track), None);
    }
}
