// Appended only to an isolated copy of HistorySyncer's main.rs by the Python
// generator. Metadata and compressed bytes come from the writer itself.
#[cfg(not(feature = "desktop"))]
fn main() {
    let destination = std::path::PathBuf::from(std::env::var_os("PE_FIXTURE_OUTPUT").unwrap());
    for (name, archive_use) in [
        ("climatology_base", layout::ArchiveUse::ClimatologyBase),
        ("routing", layout::ArchiveUse::Routing),
    ] {
        let path = destination.join(name);
        let options = layout::CreationOptions {
            mode: layout::ArchiveMode::Static,
            start_time: "2000-01-01T00:00".into(),
            stop_time: "2000-01-07T00:00".into(),
            archive_use,
        }.normalized().unwrap();
        create_archive_with_options(&path, &[
            "wind10m".into(), "currents".into(), "waves".into(), "mslp".into(), "z500".into(),
        ], &options).unwrap();
        for time_shard in 0..2 {
            // Latitude band 1 has 14 tile rows; longitude band 3 has 20 columns.
            // Tile 135 contains global cell (203, 1107): local row 3, column 7.
            let mut shard = Vec::new();
            let mut index = vec![u64::MAX; 14 * 20 * 2];
            for tile in [135, 136] {
                let mut values = Vec::new();
                for step in 0..72 {
                    let hour = (time_shard * 72 + step) as f32;
                    for field in 0..11 {
                        for y in 0..20 {
                            for x in 0..20 {
                                let (y, x) = (y as f32, x as f32);
                                let value = if tile == 136 || (x == 19. && (2..7).contains(&field)) {
                                    f32::NAN
                                } else {
                                    match field {
                                        0 => 2. + hour / 4. + y / 2. + x / 8.,
                                        1 => 4. + hour / 8. + y / 4. + x / 16.,
                                        2 => 0.5 + hour / 128. + y / 64. + x / 32.,
                                        3 => -0.25 - hour / 128. - y / 64. - x / 32.,
                                        4 => if step % 2 == 0 { 350. } else { 10. },
                                        5 => 2. + hour / 16. + y / 8. + x / 16.,
                                        6 => 8. + hour / 32. + y / 16. + x / 32.,
                                        7 => 101_325.,
                                        8 => 58_839.,
                                        9 => 12.,
                                        _ => 0.75,
                                    }
                                };
                                values.push(value);
                            }
                        }
                    }
                }
                let bytes = options.compression.encode(&values, layout::DataLayout::Compact, 72).unwrap();
                // The writer must be able to read its own fixture before it is recorded.
                let decoded = options.compression.decode(&bytes, values.len(), layout::DataLayout::Compact, 72).unwrap();
                assert_eq!(decoded.len(), values.len());
                assert!(decoded.iter().zip(&values).all(|(a, b)| a.is_nan() == b.is_nan()));
                index[tile * 2] = shard.len() as u64;
                index[tile * 2 + 1] = bytes.len() as u64;
                shard.extend(bytes);
            }
            let mut index: Vec<u8> = index.iter().flat_map(|n| n.to_le_bytes()).collect();
            index.extend(crc32c::crc32c(&index).to_le_bytes());
            shard.extend(index);
            let key = path.join(format!("data/c/{time_shard}/0/1/3"));
            std::fs::create_dir_all(key.parent().unwrap()).unwrap();
            std::fs::write(key, shard).unwrap();
        }
    }
}
