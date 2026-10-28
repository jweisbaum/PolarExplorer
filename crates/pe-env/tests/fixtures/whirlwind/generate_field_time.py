"""Regenerate synthetic field-first shards using the reference zstd CLI.

Only needed to regenerate fixtures; tests and the app need no Python or zstd CLI.
Run: python3 crates/pe-env/tests/fixtures/whirlwind/generate_field_time.py
"""
import json
from pathlib import Path
import struct
import subprocess


NAMES = ["mslp", "u10", "z500", "v10", "ucur", "vcur", "wave_direction",
         "wave_height", "wave_period", "wind_speed", "wind_constancy"]


def crc32c(data):
    crc = 0xFFFFFFFF
    for byte in data:
        crc ^= byte
        for _ in range(8):
            crc = (crc >> 1) ^ (0x82F63B78 if crc & 1 else 0)
    return crc ^ 0xFFFFFFFF


def value(name, hour, row, col):
    return {
        "u10": 2 * hour + row + col / 8,
        "v10": 4 + hour / 2 + row / 4 + col / 8,
        "ucur": 0.5 + hour / 16 + row / 32 + col / 64,
        "vcur": -0.25 - hour / 16 - row / 32 - col / 64,
        "wave_direction": 350 if hour % 2 == 0 else 10,
        "wave_height": 2 + hour + row / 8 + col / 16,
        "wave_period": 8 + hour / 2 + row / 4 + col / 8,
    }.get(name, 100 + hour + row + col)


def generate(dtype, fmt):
    root = Path(__file__).parent / f"field-time-zstd22-{dtype}"
    metadata = {
        "zarr_format": 3, "node_type": "array", "shape": [11, 4, 4, 8],
        "data_type": dtype, "fill_value": "NaN",
        "dimension_names": ["param", "time", "latitude", "longitude"],
        "chunk_key_encoding": {"name": "default", "configuration": {"separator": "/"}},
        "chunk_grid": {"name": "rectilinear", "configuration": {
            "kind": "inline", "chunk_shapes": [11, 2, [2, 2], [4, 4]]}},
        "codecs": [{"name": "sharding_indexed", "configuration": {
            "chunk_shape": [11, 2, 2, 2], "index_location": "end",
            "codecs": [{"name": "bytes", "configuration": {"endian": "little"}},
                       {"name": "zstd", "configuration": {"level": 22, "checksum": True}}],
            "index_codecs": [{"name": "bytes", "configuration": {"endian": "little"}},
                             {"name": "crc32c"}]}}],
    }
    root.mkdir(exist_ok=True)
    (root / "zarr.json").write_text(json.dumps(metadata, indent=2) + "\n")
    for time in range(2):
        for y in range(2):
            for x in range(2):
                shard, index = bytearray(), bytearray()
                for tile in range(2):
                    raw = b"".join(
                        struct.pack(fmt, value(name, time * 2 + step, y * 2 + row,
                                               x * 4 + tile * 2 + col))
                        for name in NAMES for step in range(2)
                        for row in range(2) for col in range(2)
                    )
                    encoded = subprocess.run(
                        ["zstd", "--ultra", "-22", "--check", "-q", "-c"],
                        input=raw, stdout=subprocess.PIPE, check=True,
                    ).stdout
                    assert len(encoded) < len(raw)  # genuinely compressed, not raw blocks
                    index.extend(struct.pack("<QQ", len(shard), len(encoded)))
                    shard.extend(encoded)
                shard.extend(index)
                shard.extend(struct.pack("<I", crc32c(index)))
                path = root / f"c/0/{time}/{y}/{x}"
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(shard)


if __name__ == "__main__":
    generate("float16", "<e")
    generate("float32", "<f")
