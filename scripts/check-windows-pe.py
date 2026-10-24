#!/usr/bin/env python3
"""Prove a Windows release binary is an x64 console PE that needs no Visual C++ runtime installed.

🔴 WHY THIS EXISTS. `.cargo/config.toml` asks for `+crt-static` on MSVC targets, and a `RUSTFLAGS`
environment variable REPLACES that table rather than adding to it. The release job sets
`RUSTFLAGS: -D warnings`, so a Windows build configured like the Unix ones links the C runtime
DYNAMICALLY: it imports `VCRUNTIME140.dll` plus the `api-ms-win-crt-*` UCRT forwarders. It then
builds, passes `--version` on the runner (every GitHub Windows image has the VC++ redistributable),
and fails to start on a fresh Windows install with "VCRUNTIME140.dll was not found". The runner is
exactly the machine on which that defect is invisible, so this reads the binary's OWN import table.

WHAT IT CHECKS: the MZ/PE signatures, machine AMD64, the console subsystem (hooks and the MCP stdio
server need inherited stdio), and that no imported DLL is a dynamic C/C++ runtime. WHAT IT DOES NOT
CHECK: delay-load imports, DLLs loaded at runtime with LoadLibrary, or whether the binary is CORRECT
once it starts. Exit 0 = all checks passed, 1 = a check failed, 2 = the file could not be read as a PE.
"""
from __future__ import annotations

import struct
import sys
from pathlib import Path

MACHINE_AMD64 = 0x8664
SUBSYSTEM_WINDOWS_CUI = 3
PE32_PLUS_MAGIC = 0x20B
IMPORT_DIRECTORY_INDEX = 1
IMPORT_DESCRIPTOR_SIZE = 20
#: Bounds on every loop and read below. A real import table has tens of entries, never thousands.
MAX_IMPORT_DESCRIPTORS = 4096
MAX_DLL_NAME = 260
MAX_SECTIONS = 96
MAX_BINARY_BYTES = 512 * 1024 * 1024

#: A lower-cased import name with any of these prefixes means the C runtime was linked dynamically.
DYNAMIC_CRT_PREFIXES = ("vcruntime", "msvcp", "api-ms-win-crt-", "ucrtbase")


class NotPE(ValueError):
    """The bytes are not a PE32+ image this checker can read."""


def _u16(data: bytes, offset: int) -> int:
    if offset < 0 or offset + 2 > len(data):
        raise NotPE(f"truncated at offset {offset}")
    return struct.unpack_from("<H", data, offset)[0]


def _u32(data: bytes, offset: int) -> int:
    if offset < 0 or offset + 4 > len(data):
        raise NotPE(f"truncated at offset {offset}")
    return struct.unpack_from("<I", data, offset)[0]


def _sections(data: bytes, table: int, count: int) -> list[tuple[int, int, int]]:
    """(virtual_address, size, raw_pointer) per section."""
    if not 0 < count <= MAX_SECTIONS:
        raise NotPE(f"implausible section count {count}")
    rows = []
    for index in range(count):
        base = table + index * 40
        size = max(_u32(data, base + 8), _u32(data, base + 16))
        rows.append((_u32(data, base + 12), size, _u32(data, base + 20)))
    return rows


def _offset(sections: list[tuple[int, int, int]], rva: int) -> int:
    for virtual, size, raw in sections:
        if virtual <= rva < virtual + size:
            return raw + (rva - virtual)
    raise NotPE(f"RVA {rva:#x} lies in no section")


def _cstring(data: bytes, offset: int) -> str:
    end = data.find(b"\0", offset, offset + MAX_DLL_NAME)
    if end < 0:
        raise NotPE(f"unterminated DLL name at {offset}")
    return data[offset:end].decode("ascii", "replace")


def inspect(data: bytes) -> dict:
    """Machine, subsystem and imported DLL names of a PE32+ image."""
    if data[:2] != b"MZ":
        raise NotPE("no MZ signature")
    pe = _u32(data, 0x3C)
    if data[pe:pe + 4] != b"PE\0\0":
        raise NotPE("no PE signature")
    coff = pe + 4
    machine, section_count = _u16(data, coff), _u16(data, coff + 2)
    optional_size = _u16(data, coff + 16)
    optional = coff + 20
    if _u16(data, optional) != PE32_PLUS_MAGIC:
        raise NotPE("not a PE32+ (64-bit) optional header")
    subsystem = _u16(data, optional + 68)
    import_rva = _u32(data, optional + 112 + IMPORT_DIRECTORY_INDEX * 8)
    sections = _sections(data, optional + optional_size, section_count)
    dlls = []
    if import_rva:
        cursor = _offset(sections, import_rva)
        for _ in range(MAX_IMPORT_DESCRIPTORS):
            descriptor = data[cursor:cursor + IMPORT_DESCRIPTOR_SIZE]
            if len(descriptor) < IMPORT_DESCRIPTOR_SIZE:
                raise NotPE("import table runs off the end of the file")
            if descriptor == bytes(IMPORT_DESCRIPTOR_SIZE):
                break
            dlls.append(_cstring(data, _offset(sections, _u32(data, cursor + 12))))
            cursor += IMPORT_DESCRIPTOR_SIZE
        else:
            raise NotPE(f"import table has more than {MAX_IMPORT_DESCRIPTORS} entries")
    return {"machine": machine, "subsystem": subsystem, "dlls": dlls}


def verdict(facts: dict) -> list[str]:
    """Every failed check, by name. Empty means all passed."""
    problems = []
    if facts["machine"] != MACHINE_AMD64:
        problems.append(f"machine is {facts['machine']:#x}, not AMD64 ({MACHINE_AMD64:#x})")
    if facts["subsystem"] != SUBSYSTEM_WINDOWS_CUI:
        problems.append(f"subsystem is {facts['subsystem']}, not console ({SUBSYSTEM_WINDOWS_CUI})")
    if not facts["dlls"]:
        # Absent is not zero: a real Rust exe always imports kernel32, so "no imports" means the
        # parser read the wrong place, never that the binary is clean.
        problems.append("no imports were read — the import table was not found, so nothing was checked")
    dynamic = [dll for dll in facts["dlls"] if dll.lower().startswith(DYNAMIC_CRT_PREFIXES)]
    if dynamic:
        problems.append("links the C runtime dynamically (build with -C target-feature=+crt-static): "
                        + ", ".join(dynamic))
    return problems


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: check-windows-pe.py <estelle.exe>", file=sys.stderr)
        return 2
    path = Path(argv[1])
    try:
        if path.stat().st_size > MAX_BINARY_BYTES:
            raise NotPE(f"larger than {MAX_BINARY_BYTES} bytes")
        facts = inspect(path.read_bytes())
    except (OSError, NotPE) as error:
        print(f"windows PE check: cannot inspect {path}: {error}", file=sys.stderr)
        return 2
    problems = verdict(facts)
    for problem in problems:
        print(f"windows PE check: {path.name}: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"windows PE check: {path.name} is an AMD64 console image with a static C runtime; "
          f"imports {len(facts['dlls'])} DLLs: {', '.join(sorted(facts['dlls']))}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
