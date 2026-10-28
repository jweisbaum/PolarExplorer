"""Record fixtures from a local HistorySyncer checkout's actual writer.

Usage: python3 generate_historysyncer.py /path/to/HistorySyncer
Builds an isolated source copy offline; does not edit the writer checkout.
Python, Cargo and HistorySyncer's build dependencies are only needed to regenerate.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


source = Path(sys.argv[1]).resolve()
destination = Path(__file__).resolve().parent / "historysyncer-v4"
with tempfile.TemporaryDirectory(prefix="pe-historysyncer-fixture-") as scratch:
    scratch = Path(scratch)
    shutil.copytree(source / "src", scratch / "src")
    for name in ["Cargo.toml", "Cargo.lock", "build.rs"]:
        shutil.copy2(source / name, scratch / name)
    # Reuse dependency builds without replacing HistorySyncer's executable.
    manifest = scratch / "Cargo.toml"
    manifest.write_text(manifest.read_text().replace(
        'name = "history-syncer"', 'name = "pe-historysyncer-fixture-generator"'))
    for name in ["crates", "ui", "tools"]:
        (scratch / name).symlink_to(source / name, target_is_directory=True)
    main = scratch / "src/main.rs"
    original = main.read_text()
    assert original.count("async fn main()") == 1
    main.write_text(original.replace("async fn main()", "async fn fixture_unused_worker_main()")
                    + "\n" + Path(__file__).with_suffix(".rs").read_text())
    environment = dict(os.environ, PE_FIXTURE_OUTPUT=str(scratch / "output"),
                       CARGO_INCREMENTAL="0")
    subprocess.run(["cargo", "run", "--offline", "--no-default-features",
                    "--manifest-path", str(scratch / "Cargo.toml"),
                    "--target-dir", str(source / "target")], env=environment, check=True)
    destination.mkdir(exist_ok=True)
    for preset in ["climatology_base", "routing"]:
        archive = scratch / "output" / preset
        for path in archive.rglob("*"):
            relative = path.relative_to(archive)
            if path.is_file() and ".historysyncer" not in relative.parts:
                output = destination / preset / relative
                output.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, output)
    hashes = {name: hashlib.sha256((source / name).read_bytes()).hexdigest()
              for name in ["src/main.rs", "src/compression.rs", "src/layout.rs"]}
    (destination / "writer-sha256.json").write_text(json.dumps(hashes, indent=2) + "\n")
