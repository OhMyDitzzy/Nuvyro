"""Synthetic ELF validation tests; no native compilation or device dependency."""

import struct
import unittest

from android_elf import inspect_library


def library(alignment=16384, needed=b"libc.so", runpath=False):
    data = bytearray(512)
    data[:7] = b"\x7fELF\x02\x01\x01"
    struct.pack_into("<HHI", data, 16, 3, 183, 1)
    struct.pack_into("<Q", data, 32, 64)
    struct.pack_into("<HHH", data, 52, 64, 56, 2)
    struct.pack_into("<IIQQQQQQ", data, 64, 1, 5, 0, 0, 0, 512, 512, alignment)
    dynamic = [(5, 400), (10, len(needed) + 2), (1, 1)]
    if runpath:
        dynamic.append((29, 1))
    dynamic.append((0, 0))
    struct.pack_into("<IIQQQQQQ", data, 120, 2, 6, 256, 256, 0, len(dynamic) * 16, len(dynamic) * 16, 8)
    for index, pair in enumerate(dynamic):
        struct.pack_into("<qQ", data, 256 + index * 16, *pair)
    data[400:400 + len(needed) + 2] = b"\x00" + needed + b"\x00"
    return bytes(data)


class AndroidElfTest(unittest.TestCase):
    def test_accepts_platform_dependency_and_16k_alignment(self):
        self.assertEqual(inspect_library(library())["needed"], ["libc.so"])

    def test_rejects_4k_alignment(self):
        with self.assertRaisesRegex(ValueError, "16 KiB"):
            inspect_library(library(alignment=4096))

    def test_rejects_termux_dependency(self):
        with self.assertRaisesRegex(ValueError, "Non-platform dependency"):
            inspect_library(library(needed=b"libandroid-support.so"))

    def test_rejects_runpath(self):
        with self.assertRaisesRegex(ValueError, "RPATH/RUNPATH"):
            inspect_library(library(runpath=True))

    def test_rejects_wrong_abi_and_truncated_file(self):
        wrong = bytearray(library())
        struct.pack_into("<H", wrong, 18, 62)
        with self.assertRaisesRegex(ValueError, "AArch64"):
            inspect_library(bytes(wrong))
        with self.assertRaises(ValueError):
            inspect_library(library()[:80])


if __name__ == "__main__":
    unittest.main()
