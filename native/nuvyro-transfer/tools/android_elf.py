"""Validate the built arm64 JNI artifact before copying it into Android outputs."""

import struct


ALLOWED_NEEDED = {"libandroid.so", "libc.so", "libdl.so", "liblog.so", "libm.so"}


def inspect_library(data):
    if len(data) < 64 or data[:7] != b"\x7fELF\x02\x01\x01":
        raise ValueError("Expected a little-endian ELF64 library")
    if struct.unpack_from("<HH", data, 16) != (3, 183):
        raise ValueError("Expected an AArch64 shared library")
    offset = struct.unpack_from("<Q", data, 32)[0]
    entry_size, count = struct.unpack_from("<HH", data, 54)
    if entry_size != 56 or not 1 <= count <= 128 or offset + count * entry_size > len(data):
        raise ValueError("Invalid ELF program headers")
    loads, dynamic = [], None
    for index in range(count):
        kind, flags, position, address, physical, size, memory, alignment = struct.unpack_from(
            "<IIQQQQQQ", data, offset + index * entry_size
        )
        if position + size > len(data):
            raise ValueError("ELF segment exceeds file")
        if kind == 1:
            if size > memory or alignment < 16384 or alignment & (alignment - 1) or (position - address) % 16384:
                raise ValueError("PT_LOAD is not compatible with 16 KiB alignment")
            loads.append((address, position, size))
        elif kind == 2:
            if dynamic is not None or size % 16:
                raise ValueError("Invalid dynamic segment")
            dynamic = (position, size)
    if not loads or dynamic is None:
        raise ValueError("Shared library has no load/dynamic segments")
    string_address, string_size, needed = None, None, []
    terminated = False
    start, size = dynamic
    for cursor in range(start, start + size, 16):
        tag, value = struct.unpack_from("<qQ", data, cursor)
        if tag == 0:
            terminated = True
            break
        if tag in (15, 29):
            raise ValueError("RPATH/RUNPATH is not allowed in the Android library")
        if tag == 5:
            string_address = value
        elif tag == 10:
            string_size = value
        elif tag == 1:
            needed.append(value)
    if not terminated or string_address is None or string_size is None or string_size > 65536:
        raise ValueError("Invalid dynamic string table")
    table = None
    for address, position, size in loads:
        if address <= string_address and string_address + string_size <= address + size:
            begin = position + string_address - address
            table = data[begin:begin + string_size]
            break
    if table is None:
        raise ValueError("Unmapped dynamic string table")
    libraries = []
    for index in needed:
        if index >= len(table):
            raise ValueError("Invalid dependency name")
        end = table.find(b"\x00", index)
        if end < 0:
            raise ValueError("Unterminated dependency name")
        name = table[index:end].decode("ascii", errors="strict")
        if name not in ALLOWED_NEEDED:
            raise ValueError(f"Non-platform dependency must not ship implicitly: {name}")
        libraries.append(name)
    return {"abi": "arm64-v8a", "load_alignment": 16384, "needed": libraries}
