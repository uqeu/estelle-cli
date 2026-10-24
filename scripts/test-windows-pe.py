#!/usr/bin/env python3
"""Prove check-windows-pe.py can FAIL, on synthetic PE images built here byte by byte.

The checker only ever runs in CI against the real estelle.exe, which (when built right) passes. A
checker that only ever sees passing input has never been shown to see anything, so this builds the
minimal PE32+ layout it parses and drives every verdict arm: clean, dynamic CRT, wrong machine, GUI
subsystem, no import table, and non-PE bytes. Runs on any host; needs no Windows toolchain.
"""
from __future__ import annotations

import importlib.util
import struct
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("check_windows_pe", HERE / "check-windows-pe.py")
assert SPEC is not None and SPEC.loader is not None
PE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PE)

SECTION_RVA = 0x1000
SECTION_RAW = 0x200
OPTIONAL_SIZE = 240


def build(dlls: list[str], machine: int = 0x8664, subsystem: int = 3, with_imports: bool = True) -> bytes:
    """A minimal PE32+ with one section holding an import table that names `dlls`."""
    names_at = SECTION_RVA + (len(dlls) + 1) * 20
    table, blob = b"", b""
    for dll in dlls:
        table += struct.pack("<IIIII", 0, 0, 0, names_at + len(blob), 0)
        blob += dll.encode("ascii") + b"\0"
    section = table + bytes(20) + blob
    dos = bytearray(0x40)
    dos[:2] = b"MZ"
    struct.pack_into("<I", dos, 0x3C, 0x40)
    coff = struct.pack("<HHIIIHH", machine, 1, 0, 0, 0, OPTIONAL_SIZE, 0x22)
    optional = bytearray(OPTIONAL_SIZE)
    struct.pack_into("<H", optional, 0, 0x20B)
    struct.pack_into("<H", optional, 68, subsystem)
    struct.pack_into("<I", optional, 108, 16)
    if with_imports:
        struct.pack_into("<II", optional, 112 + 8, SECTION_RVA, len(table) + 20)
    header = struct.pack("<8sIIII", b".idata", len(section), SECTION_RVA, len(section), SECTION_RAW)
    header += bytes(40 - len(header))
    image = bytes(dos) + b"PE\0\0" + coff + bytes(optional) + header
    assert len(image) <= SECTION_RAW, "headers overflowed the first section"
    return image + bytes(SECTION_RAW - len(image)) + section


def run(data: bytes) -> int:
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "estelle.exe"
        path.write_bytes(data)
        return PE.main(["check-windows-pe.py", str(path)])


def main() -> None:
    static = ["KERNEL32.dll", "ntdll.dll", "WS2_32.dll", "bcrypt.dll"]
    assert PE.inspect(build(static))["dlls"] == static, "the parser must read back what was written"
    cases = {
        "clean static-CRT console exe": (build(static), 0),
        "dynamic VCRUNTIME140": (build(static + ["VCRUNTIME140.dll"]), 1),
        "dynamic UCRT forwarder": (build(static + ["api-ms-win-crt-runtime-l1-1-0.dll"]), 1),
        "ARM64 machine": (build(static, machine=0xAA64), 1),
        "GUI subsystem (no inherited stdio)": (build(static, subsystem=2), 1),
        "no import table read": (build(static, with_imports=False), 1),
        "an ELF, not a PE": (b"\x7fELF" + bytes(256), 2),
        "truncated PE": (build(static)[:0x60], 2),
    }
    for name, (data, expected) in cases.items():
        actual = run(data)
        assert actual == expected, f"{name}: exit {actual}, expected {expected}"
    print(f"windows PE check proof: {len(cases)} cases, every verdict arm reached "
          "(1 pass, 5 refusals, 2 unreadable)")


if __name__ == "__main__":
    main()
    sys.exit(0)
