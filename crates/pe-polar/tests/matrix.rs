#![allow(clippy::expect_used, clippy::unwrap_used, reason = "test assertions")]

use pe_polar::{GaussianPdf, MatrixPolarExt, WhirlwindMatrixPolar, matrix};

fn example(directions: u32) -> WhirlwindMatrixPolar {
    let cells = if directions == 0 {
        4
    } else {
        2 * directions as usize * 2 * 4
    };
    WhirlwindMatrixPolar {
        twa_deg: vec![45.0, 135.0],
        tws_kn: vec![8.0, 16.0],
        wave_height_cutoffs_m: if directions == 0 { vec![] } else { vec![1.25] },
        wave_direction_count: directions,
        wave_period_cutoffs_s: if directions == 0 { vec![] } else { vec![5.5] },
        sog_bins_kn: vec![0.5, 1.5],
        sog: Some(
            (0..cells)
                .map(|k| (k % 7 != 0).then_some((k % 60) as f32))
                .collect(),
        ),
        pdf: Some((0..cells * 2).map(|k| (k % 11) as f32 / 10.0).collect()),
        pdf_gaussians: Some(
            (0..cells)
                .map(|k| {
                    (k % 7 != 0).then_some(GaussianPdf {
                        mean_kn: (k % 60) as f32,
                        std_dev_kn: 1.5,
                    })
                })
                .collect(),
        ),
    }
}

#[test]
fn every_supported_direction_count_and_mode_combination_round_trips() {
    for directions in [0, 2, 4, 8, 20] {
        for modes in 1..=7 {
            let mut input = example(directions);
            if modes & 1 == 0 {
                input.sog = None;
            }
            if modes & 2 == 0 {
                input.pdf = None;
                input.sog_bins_kn.clear();
            }
            if modes & 4 == 0 {
                input.pdf_gaussians = None;
            }
            let bytes = matrix::write(&input).unwrap();
            assert_eq!(&bytes[8..10], &1u16.to_le_bytes());
            let output = matrix::read(&bytes).unwrap();
            assert_eq!(output.wave_direction_count, directions);
            assert_eq!(output.wave_height_cutoffs_m, input.wave_height_cutoffs_m);
            assert_eq!(output.wave_period_cutoffs_s, input.wave_period_cutoffs_s);
            assert_eq!(output.twa_deg, input.twa_deg);
            assert_eq!(output.tws_kn, input.tws_kn);
            assert_eq!(output.sog_bins_kn, input.sog_bins_kn);
            assert_eq!(output.sog.is_some(), input.sog.is_some());
            assert_eq!(output.pdf.is_some(), input.pdf.is_some());
            assert_eq!(
                output.pdf_gaussians.is_some(),
                input.pdf_gaussians.is_some()
            );
            if let (Some(a), Some(b)) = (&input.sog, &output.sog) {
                assert_eq!(a.len(), b.len());
                for (a, b) in a.iter().zip(b) {
                    assert_eq!(a.is_some(), b.is_some());
                    if let (Some(a), Some(b)) = (a, b) {
                        assert!((a - b).abs() < 0.001);
                    }
                }
            }
            if let (Some(a), Some(b)) = (&input.pdf, &output.pdf) {
                assert_eq!(a.len(), b.len());
                assert!(a.iter().zip(b).all(|(a, b)| (a - b).abs() < 0.0001));
            }
            if let (Some(a), Some(b)) = (&input.pdf_gaussians, &output.pdf_gaussians) {
                assert_eq!(a.len(), b.len());
                for (a, b) in a.iter().zip(b) {
                    assert_eq!(a.is_some(), b.is_some());
                    if let (Some(a), Some(b)) = (a, b) {
                        assert!((a.mean_kn - b.mean_kn).abs() < 0.001);
                        assert!((a.std_dev_kn - b.std_dev_kn).abs() < 0.001);
                    }
                }
            }
        }
    }
}

#[test]
fn wire_metadata_contains_bucket_counts_and_only_interior_cutoffs() {
    let bytes = matrix::write(&example(4)).unwrap();
    let counts: Vec<_> = bytes[12..36]
        .chunks_exact(4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .collect();
    assert_eq!(counts, [2, 2, 2, 4, 2, 2]);
    let metadata: Vec<_> = bytes[44..76]
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect();
    assert_eq!(metadata, [45.0, 135.0, 8.0, 16.0, 1.25, 5.5, 0.5, 1.5]);
    // No direction coordinates, implicit zero, or infinity bytes are stored.
    assert_eq!(
        u32::from_le_bytes(bytes[76..80].try_into().unwrap()),
        4 * 2 * 4 * 2 * 2 * 5
    );
}

#[test]
fn single_all_values_height_and_period_buckets_need_no_cutoff_bytes() {
    let mut input = example(2);
    input.wave_height_cutoffs_m.clear();
    input.wave_period_cutoffs_s.clear();
    input.sog = Some(vec![Some(10.0); 8]);
    input.pdf = None;
    input.pdf_gaussians = None;
    input.sog_bins_kn.clear();
    let bytes = matrix::write(&input).unwrap();
    assert_eq!(u32::from_le_bytes(bytes[20..24].try_into().unwrap()), 1);
    assert_eq!(u32::from_le_bytes(bytes[28..32].try_into().unwrap()), 1);
    let output = matrix::read(&bytes).unwrap();
    assert_eq!(output.wave_height_count(), 1);
    assert_eq!(output.wave_period_count(), 1);
    assert_eq!(output.sog.unwrap().len(), 8);
}

#[test]
fn reader_rejects_invalid_counts_cutoffs_and_old_versions() {
    let good = matrix::write(&example(4)).unwrap();
    for count in [1, 3, 6, 16, 18, 21, u32::MAX] {
        let mut bad = good.clone();
        bad[24..28].copy_from_slice(&count.to_le_bytes());
        assert!(matrix::read(&bad).is_err(), "direction count {count}");
    }
    for (offset, value) in [(20, 0u32), (28, 65), (24, 0)] {
        let mut bad = good.clone();
        bad[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        assert!(matrix::read(&bad).is_err());
    }
    for (offset, limit) in [(60, 100.0f32), (64, 1000.0)] {
        for value in [0.0, -1.0, limit, f32::NAN, f32::INFINITY] {
            let mut bad = good.clone();
            bad[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert!(matrix::read(&bad).is_err(), "cutoff {value}");
        }
    }
    for version in [0u16, 2, 3] {
        let mut bad = good.clone();
        bad[8..10].copy_from_slice(&version.to_le_bytes());
        assert!(matches!(matrix::read(&bad), Err(matrix::MatrixError::Version(v)) if v == version));
    }
    for length in 0..good.len() {
        assert!(
            matrix::read(&good[..length]).is_err(),
            "truncated to {length}"
        );
    }
}

#[test]
fn writer_rejects_invalid_or_disabled_wave_buckets() {
    for count in [1, 3, 16, 18, 36] {
        let mut input = example(4);
        input.wave_direction_count = count;
        assert!(matrix::write(&input).is_err());
    }
    for cutoffs in [
        vec![1.0, 1.0],
        vec![2.0, 1.0],
        vec![0.0],
        vec![100.0],
        vec![f32::NAN],
        vec![1.0; 64],
    ] {
        let mut input = example(4);
        input.wave_height_cutoffs_m = cutoffs;
        assert!(matrix::write(&input).is_err());
    }
    let mut input = example(0);
    input.wave_height_cutoffs_m = vec![1.0];
    assert!(matrix::write(&input).is_err());
}

#[test]
fn full_circle_wave_slices_preserve_order_without_mirroring_wind() {
    let input = example(4);
    let decoded = matrix::read(&matrix::write(&input).unwrap()).unwrap();
    let polar = decoded.to_polar().unwrap();
    assert_eq!(polar.twa, [45.0, 135.0]);
    for t in 0..2 {
        for w in 0..2 {
            let values: Vec<_> = (0..16)
                .filter_map(|wave| input.sog.as_ref().unwrap()[wave * 4 + t * 2 + w])
                .collect();
            let expected = values.iter().sum::<f32>() / values.len() as f32;
            assert!((polar.get(t, w).unwrap() - f64::from(expected)).abs() < 0.001);
        }
    }
}
