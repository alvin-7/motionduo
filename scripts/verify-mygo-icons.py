#!/usr/bin/env python3
"""Inspect PE icon resources without launching or installing the application."""

import argparse
import hashlib
import json
from pathlib import Path
import struct


ROOT = Path(__file__).resolve().parents[1]


def verify(binary, source):
    data, ico = binary.read_bytes(), source.read_bytes()
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        raise ValueError("Not a PE executable")
    sections, optional_size = struct.unpack_from("<H", data, pe + 6)[0], struct.unpack_from("<H", data, pe + 20)[0]
    optional = pe + 24
    magic = struct.unpack_from("<H", data, optional)[0]
    directories = optional + {0x20B: 112, 0x10B: 96}[magic]
    resource_rva = struct.unpack_from("<I", data, directories + 16)[0]
    if not resource_rva:
        raise ValueError("Executable has no Windows resource directory")

    def file_offset(rva):
        for i in range(sections):
            at = optional + optional_size + i * 40
            virtual_size, virtual_address, raw_size, raw_at = struct.unpack_from("<IIII", data, at + 8)
            if virtual_address <= rva < virtual_address + max(virtual_size, raw_size):
                return raw_at + rva - virtual_address
        raise ValueError(f"Unmapped resource RVA {rva}")

    root = file_offset(resource_rva)

    def entries(relative):
        named, numbered = struct.unpack_from("<HH", data, root + relative + 12)
        return [struct.unpack_from("<II", data, root + relative + 16 + 8 * i) for i in range(named + numbered)]

    resources = {}
    for kind, directory in entries(0):
        if kind not in (3, 14):
            continue
        for identity, languages in entries(directory & 0x7FFFFFFF):
            for _, leaf in entries(languages & 0x7FFFFFFF):
                rva, length = struct.unpack_from("<II", data, root + leaf)
                at = file_offset(rva)
                resources[kind, identity] = data[at:at + length]

    expected = []
    for i in range(struct.unpack_from("<H", ico, 4)[0]):
        length, offset = struct.unpack_from("<II", ico, 6 + i * 16 + 8)
        expected.append(hashlib.sha256(ico[offset:offset + length]).hexdigest())
    groups = []
    for (kind, identity), group in resources.items():
        if kind != 14:
            continue
        actual = []
        sizes = []
        for i in range(struct.unpack_from("<H", group, 4)[0]):
            width, height, _, _, _, _, length, image_id = struct.unpack_from("<BBBBHHIH", group, 6 + i * 14)
            payload = resources[3, image_id]
            if len(payload) != length:
                raise ValueError("Truncated icon resource")
            actual.append(hashlib.sha256(payload).hexdigest())
            sizes.append([width or 256, height or 256])
        groups.append({"id": identity, "sizes": sizes, "matches_source": sorted(actual) == sorted(expected)})
    if not groups or not all(group["matches_source"] for group in groups):
        raise ValueError("Application icon does not match the MotionDuo ICO")
    return {"binary": str(binary), "sha256": hashlib.sha256(data).hexdigest(), "groups": groups, "result": "pass"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/motionduo-ui.exe")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    result = verify(args.binary.resolve(), ROOT / "assets/brand/motionduo.ico")
    text = json.dumps(result, indent=2)
    if args.output:
        args.output.write_text(text, encoding="utf-8")
    print(text)


if __name__ == "__main__":
    main()
