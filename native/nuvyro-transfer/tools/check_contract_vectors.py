"""Check public encoding fixtures, without executing the Rust implementation."""

import base64
import binascii
import json
from pathlib import Path
import re
import struct


FIXTURES = (
    Path(__file__).resolve().parents[1]
    / "crates/nuvyro-transfer-core/tests/fixtures"
)
BODY_BOUNDS = {
    0x0001: (24, 24),
    0x0002: (24, 24),
    0x0003: (51, 178),
    0x0004: (32, 32),
    0x0100: (80, 61508),
    0x0101: (64, 64),
    0x0102: (66, 66),
    0x0200: (56, 56),
    0x0201: (56, 56),
    0x0202: (57, 262200),
    0x0203: (88, 88),
    0x0204: (88, 88),
    0x0205: (107, 361),
    0x0300: (74, 74),
    0x0301: (74, 74),
    0x0302: (57, 57),
    0x7F00: (34, 34),
    0x7FFF: (35, 35),
}
QR_PREFIX = "NUVYRO:PAIR:1:"
MIME = re.compile(r"[A-Za-z0-9!#$&^_.+-]+/[A-Za-z0-9!#$&^_.+-]+", re.ASCII)


class InvalidFixtureValue(Exception):
    pass


def require(condition, outcome):
    if not condition:
        raise InvalidFixtureValue(outcome)


def header_outcome(raw):
    assert len(raw) == 6
    length, kind = struct.unpack(">IH", raw)
    if length < 2 or length > 262202:
        return "length"
    if kind not in BODY_BOUNDS:
        return "unknown"
    minimum, maximum = BODY_BOUNDS[kind]
    if (kind != 0x0202 and length > 65536) or not minimum <= length - 2 <= maximum:
        return "length"
    return f"ok:{kind:04x}:{length - 2}"


def check_qr(value):
    require(len(value.encode("utf-8")) <= 512, "limit")
    require(value.startswith(QR_PREFIX), "prefix")
    encoded = value[len(QR_PREFIX):]
    require(re.fullmatch(r"[A-Za-z0-9_-]+", encoded) is not None, "encoding")
    try:
        decoded = base64.b64decode(
            encoded + "=" * (-len(encoded) % 4), altchars=b"-_", validate=True
        )
    except binascii.Error as error:
        raise InvalidFixtureValue("encoding") from error
    require(base64.urlsafe_b64encode(decoded).rstrip(b"=").decode() == encoded, "encoding")
    require(len(decoded) == 104, "length")
    require(struct.unpack(">HH", decoded[:4]) == (1, 0), "version")
    require(struct.unpack(">H", decoded[100:102])[0] == 47739, "port")
    require(struct.unpack(">H", decoded[102:104])[0] == 120, "ttl")
    for start, end in ((4, 20), (20, 36), (36, 68), (68, 100)):
        require(any(decoded[start:end]), "zero")


def check_manifest(raw):
    require(len(raw) <= 61440, "LimitExceeded")
    cursor = 0

    def take(size):
        nonlocal cursor
        require(cursor + size <= len(raw), "MalformedFrame")
        result = raw[cursor:cursor + size]
        cursor += size
        return result

    def text(maximum):
        length = struct.unpack(">H", take(2))[0]
        require(0 < length <= maximum, "LimitExceeded")
        try:
            return take(length).decode("utf-8", errors="strict")
        except UnicodeDecodeError as error:
            raise InvalidFixtureValue("MalformedFrame") from error

    schema, count, total = struct.unpack(">HHQ", take(12))
    require(schema == 1 and count > 0, "MalformedFrame")
    require(count <= 128 and total <= 1 << 42, "LimitExceeded")
    identifiers = set()
    sizes = []
    for _ in range(count):
        identifier = take(16)
        require(any(identifier), "MalformedFrame")
        require(identifier not in identifiers, "DuplicateId")
        identifiers.add(identifier)
        size = struct.unpack(">Q", take(8))[0]
        require(size <= 1 << 40, "LimitExceeded")
        name = text(255)
        require(name.strip() and name not in (".", ".."), "UnsafeName")
        require(not any(
            ord(char) < 32 or 127 <= ord(char) <= 159
            or 0x202A <= ord(char) <= 0x202E or 0x2066 <= ord(char) <= 0x2069
            or char in "/\\" for char in name
        ), "UnsafeName")
        require(MIME.fullmatch(text(128)) is not None, "MalformedFrame")
        sizes.append(size)
        require(sum(sizes) <= 1 << 42, "LimitExceeded")
    require(cursor == len(raw) and sum(sizes) == total, "MalformedFrame")
    return [str(size) for size in sizes]


def outcome(function, value):
    try:
        result = function(value)
    except InvalidFixtureValue as error:
        return str(error), None
    return "ok", result


def main():
    vector_count = 0
    for line in (FIXTURES / "frame-headers.txt").read_text().splitlines():
        if not line or line.startswith("#"):
            continue
        name, encoded, expected = line.split()
        actual = header_outcome(bytes.fromhex(encoded))
        assert actual == expected, (name, actual, expected)
        vector_count += 1

    vectors = json.loads((FIXTURES / "contract-vectors.json").read_text())
    assert vectors["schema"] == 1 and vectors["wire_version"] == [1, 0]
    names = set()
    for category, function, field in (
        ("qr_cases", check_qr, "payload"),
        ("manifest_cases", check_manifest, "hex"),
    ):
        for case in vectors[category]:
            assert case["name"] not in names
            names.add(case["name"])
            value = bytes.fromhex(case[field]) if field == "hex" else case[field]
            actual, result = outcome(function, value)
            assert actual == case["expect"], (case["name"], actual, case["expect"])
            if "expected_sizes" in case:
                assert result == case["expected_sizes"]

    for example in vectors["frame_examples"]:
        raw = bytes.fromhex(example["hex"])
        length, kind = struct.unpack(">IH", raw[:6])
        assert len(raw) == length + 4 and kind == example["kind"]
        assert header_outcome(raw[:6]).startswith("ok:")

    print(f"Header fixtures: {vector_count} checked against independent Python oracle")
    print(f"QR fixtures: {len(vectors['qr_cases'])}; manifest fixtures: {len(vectors['manifest_cases'])}")
    print(f"Full-frame encoding examples: {len(vectors['frame_examples'])}")
    print("No Rust execution, TLS, JNI, BLAKE3 calculation, storage, or device test performed.")


if __name__ == "__main__":
    main()
