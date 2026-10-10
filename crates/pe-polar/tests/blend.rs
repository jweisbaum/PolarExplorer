#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! The blend and its export against golden bytes (spec.md 12.3, invariant 5).
//!
//! A fixed blend — the Adrena and Expedition golden polars and a track
//! segment of forty samples, on the default output grid — is written in all
//! three formats and compared with `tests/golden/blend.*`. Those files pin
//! the export: changing them is changing export output (CLAUDE.md, "Changing
//! export output"). Two cells are checked against values worked out by hand,
//! so the golden files are not only "what the code wrote".
//!
//! And every export reads back: whatever grid is written — any axes a
//! person may give, any values — the file re-imports to the same grid, cell
//! for cell, as written with two decimals (plan.md M14, carried from M4).

use pe_core::project::OutputGrid;
use pe_core::source::Overlay;
use pe_core::track::SegmentStatistic;
use pe_polar::blend::on_grid;
use pe_polar::{
    Blend, BlendOptions, BlendSource, Confidence, Polar, PolarFileFormat, bin, blend, export, read,
};
use proptest::prelude::*;

const EXPEDITION: &[u8] = include_bytes!("golden/expedition.txt");
const ADRENA: &[u8] = include_bytes!("golden/adrena.pol");
const BLEND_TXT: &[u8] = include_bytes!("golden/blend.txt");
const BLEND_POL: &[u8] = include_bytes!("golden/blend.pol");
const BLEND_CSV: &[u8] = include_bytes!("golden/blend.csv");

const FORMATS: [PolarFileFormat; 3] = [
    PolarFileFormat::Expedition,
    PolarFileFormat::Adrena,
    PolarFileFormat::Csv,
];

fn fixed_blend() -> Blend {
    let grid = OutputGrid::default();
    let adrena = on_grid(
        &read(ADRENA).unwrap().polar,
        &Overlay::default(),
        &grid.twa,
        &grid.tws,
    );
    let expedition = on_grid(
        &read(EXPEDITION).unwrap().polar,
        &Overlay::default(),
        &grid.twa,
        &grid.tws,
    );
    // Forty samples at 90° in 10 kn, 7.00 to 7.39 kn, and ten at 120° in
    // 16 kn (below full confidence).
    let points = (0..40)
        .map(|k| (90.0, 10.0, 7.0 + f64::from(k) / 100.0))
        .chain((0..10).map(|k| (240.0, 16.2, 8.0 + f64::from(k) / 10.0)));
    let segment = bin(points, &grid.twa, &grid.tws, SegmentStatistic::P90, 5);
    let no_overrides = vec![vec![false; grid.tws.len()]; grid.twa.len()];
    let sources = [
        BlendSource {
            id: 3,
            grid: &segment.polar,
            weight: 1.0,
            confidence: Confidence::Samples {
                count: &segment.count,
                overridden: &no_overrides,
            },
        },
        BlendSource {
            id: 1,
            grid: &adrena,
            weight: 1.0,
            confidence: Confidence::Full,
        },
        BlendSource {
            id: 2,
            grid: &expedition,
            weight: 0.5,
            confidence: Confidence::Full,
        },
    ];
    blend(
        &grid.twa,
        &grid.tws,
        &sources,
        &BlendOptions {
            polar_statistic: Default::default(),
            n_full: 30,
            smoothing: false,
        },
    )
}

fn cell(polar: &Polar, twa: f64, tws: f64) -> Option<f64> {
    let i = polar.twa.iter().position(|a| *a == twa)?;
    let j = polar.tws.iter().position(|s| *s == tws)?;
    polar.get(i, j)
}

/// Worked by hand. At 90°, 10 kn: Adrena 7.66 (weight 1), Expedition 7.55
/// (weight 0.5), the track's 90th percentile of 7.00…7.39 — rank 35.1,
/// 7.35 + 0.1 × 0.01 = 7.351 — with 40 samples, over n_full 30, so
/// confidence 1: (7.66 + 3.775 + 7.351) / 2.5 = 7.5144.
///
/// At 120°, 16 kn: Adrena 10.66, Expedition 8.70 (weight 0.5), and the
/// track's ten samples 8.0…8.9 (120° folded from 240°, 16.2 kn binned to 16):
/// rank 8.1, 8.8 + 0.1 × 0.1 = 8.81, confidence 10 / 30. So
/// (10.66 + 4.35 + 8.81 / 3) / (1 + 0.5 + 1 / 3) = 17.946667 / 1.833333
/// = 9.789091.
#[test]
fn the_fixed_blend_matches_hand_worked_cells() {
    let blend = fixed_blend();
    let close = |a: Option<f64>, b: f64| {
        let a = a.unwrap();
        assert!((a - b).abs() < 1e-6, "{a} != {b}");
    };
    close(cell(&blend.polar, 90.0, 10.0), 7.5144);
    close(cell(&blend.polar, 120.0, 16.0), 9.789091);
    // The 0° row is 0 kn.
    assert!(blend.polar.bsp[0].iter().all(|v| *v == Some(0.0)));
    // No source reaches 4 kn or 25–30 kn, and nothing is extrapolated.
    assert_eq!(cell(&blend.polar, 90.0, 4.0), None);
    assert_eq!(cell(&blend.polar, 90.0, 30.0), None);
}

/// The golden bytes, in all three formats; and each reads back as the
/// grid written.
#[test]
fn the_fixed_blend_exports_its_golden_bytes() {
    let blend = fixed_blend();
    for (format, golden) in FORMATS.into_iter().zip([BLEND_TXT, BLEND_POL, BLEND_CSV]) {
        let text = export(format, &blend.polar).unwrap();
        if std::env::var_os("PE_WRITE_GOLDEN").is_some() {
            let name = match format {
                PolarFileFormat::Expedition => "blend.txt",
                PolarFileFormat::Adrena => "blend.pol",
                PolarFileFormat::Csv => "blend.csv",
            };
            std::fs::write(
                format!("{}/tests/golden/{name}", env!("CARGO_MANIFEST_DIR")),
                &text,
            )
            .unwrap();
            continue;
        }
        assert_eq!(
            text,
            std::str::from_utf8(golden).unwrap(),
            "{format:?} export drifted from its golden file"
        );
        assert_reads_back(format, &blend.polar, &text);
    }
}

/// What a writer puts down for a boat speed, read as a number: the
/// reference the re-import is held to.
fn as_written(value: f64) -> f64 {
    format!("{value:.2}").parse().unwrap()
}

/// The file reads back as the grid written: every cell with a value, at
/// the axis values written, and nothing else. Table formats keep every
/// axis value; Expedition writes only the angles a wind speed has.
fn assert_reads_back(format: PolarFileFormat, polar: &Polar, text: &str) {
    let parsed = read(text.as_bytes()).unwrap();
    assert_eq!(parsed.format, format);
    let back = parsed.polar;
    let mut expected = 0;
    for (i, twa) in polar.twa.iter().enumerate() {
        for (j, tws) in polar.tws.iter().enumerate() {
            let Some(bsp) = polar.get(i, j) else { continue };
            expected += 1;
            assert_eq!(
                cell(&back, as_written(*twa), as_written(*tws)),
                Some(as_written(bsp)),
                "{format:?} at {twa}°, {tws} kn"
            );
        }
    }
    assert_eq!(pe_polar::cell_count(&back), expected, "{format:?}");
    if format != PolarFileFormat::Expedition {
        let written = |axis: &[f64]| axis.iter().map(|v| as_written(*v)).collect::<Vec<_>>();
        assert_eq!(back.twa, written(&polar.twa));
        assert_eq!(back.tws, written(&polar.tws));
    }
}

/// An axis as the grid editor accepts it: hundredths, strictly increasing.
fn axis(max_hundredths: u32) -> impl Strategy<Value = Vec<f64>> {
    proptest::collection::btree_set(0..=max_hundredths, 1..24)
        .prop_map(|set| set.into_iter().map(|h| f64::from(h) / 100.0).collect())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    /// Every export re-imports to the same grid (plan.md M14).
    #[test]
    fn every_export_reads_back_as_the_grid_written(
        twa in axis(18_000),
        tws in axis(7_000),
        seed in proptest::collection::vec(proptest::option::of(0.0f64..60.0), 1..600),
    ) {
        let mut polar = Polar::empty(twa, tws);
        let mut values = seed.iter().cycle();
        for row in &mut polar.bsp {
            for value in row.iter_mut() {
                *value = values.next().copied().flatten();
            }
        }
        if pe_polar::cell_count(&polar) == 0 {
            polar.bsp[0][0] = Some(1.0);
        }
        for format in FORMATS {
            let text = export(format, &polar).unwrap();
            assert_reads_back(format, &polar, &text);
        }
    }
}

#[test]
fn polar_statistics_ignore_track_confidence_and_report_matching_contributions() {
    use pe_core::project::PolarStatistic::{Max, Mean, Median, Min, P90};
    use pe_polar::blend::contributions;
    let grids: Vec<_> = [2.0, 4.0, 10.0, 50.0]
        .into_iter()
        .map(|speed| Polar {
            twa: vec![90.0],
            tws: vec![10.0],
            bsp: vec![vec![Some(speed)]],
        })
        .collect();
    let sources: Vec<_> = grids
        .iter()
        .enumerate()
        .map(|(i, grid)| BlendSource {
            id: i as u64,
            grid,
            weight: if i == 3 { 0.0 } else { 1.0 },
            confidence: Confidence::Full,
        })
        .collect();
    for (statistic, expected) in [
        (Min, 2.0),
        (Median, 4.0),
        (Mean, 5.333333),
        (Max, 10.0),
        (P90, 8.8),
    ] {
        for n_full in [1, 1_000_000] {
            let options = BlendOptions {
                polar_statistic: statistic,
                n_full,
                smoothing: false,
            };
            let result = blend(&[90.0], &[10.0], &sources, &options);
            assert_eq!(result.polar.get(0, 0), Some(expected));
            let terms = contributions(&[90.0], &[10.0], &sources, &options, 0, 0);
            let weight: f64 = terms.iter().map(|term| term.weight).sum();
            let speed: f64 = terms.iter().map(|term| term.weight * term.bsp).sum::<f64>() / weight;
            assert!((speed - expected).abs() < 0.001);
            assert_eq!(weight, 3.0);
        }
    }
    let options = BlendOptions {
        polar_statistic: Median,
        n_full: 30,
        smoothing: false,
    };
    assert_eq!(
        blend(&[90.0], &[10.0], &sources[..2], &options)
            .polar
            .get(0, 0),
        Some(3.0)
    );
    assert_eq!(
        blend(&[90.0], &[10.0], &sources[..1], &options)
            .polar
            .get(0, 0),
        Some(2.0)
    );
    assert_eq!(blend(&[90.0], &[10.0], &[], &options).polar.get(0, 0), None);
    let count = vec![vec![15]];
    let overridden = vec![vec![false]];
    let mut mixed = sources[..2].to_vec();
    mixed.push(BlendSource {
        id: 9,
        grid: &grids[2],
        weight: 1.0,
        confidence: Confidence::Samples {
            count: &count,
            overridden: &overridden,
        },
    });
    let options = BlendOptions {
        polar_statistic: Max,
        ..options
    };
    assert_eq!(
        blend(&[90.0], &[10.0], &mixed, &options).polar.get(0, 0),
        Some(5.2)
    );
    assert_eq!(
        blend(&[90.0], &[10.0], &mixed[2..], &options)
            .polar
            .get(0, 0),
        Some(10.0)
    );
    mixed.reverse();
    assert_eq!(
        blend(&[90.0], &[10.0], &mixed, &options).polar.get(0, 0),
        Some(5.2)
    );
}
