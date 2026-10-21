#!/usr/bin/env python3
"""Explicitly generate new ABI6 slice transport samples from a built native library.

Not run by CI. The shared samples pin transport interpretation, with Tier3
coordinate tolerances and a zero hash sentinel, not cross-platform world hashes.
"""
import ctypes as c
from pathlib import Path
import struct
import sys


class Utc(c.Structure):
    _fields_ = [("abi", c.c_uint32), ("size", c.c_uint32), ("year", c.c_int32)] + [
        (name, c.c_uint32) for name in ("month", "day", "hour", "minute", "second", "micro", "reserved")
    ]


class Config(c.Structure):
    _fields_ = [("abi", c.c_uint32), ("size", c.c_uint32), ("seed", c.c_uint64),
                ("start", c.c_int64), ("reserved", c.c_uint64)]


class Operation(c.Structure):
    _fields_ = [("abi", c.c_uint32), ("size", c.c_uint32), ("tag", c.c_uint32),
                ("reserved", c.c_uint32), ("words", c.c_uint64 * 6)]


class Sky(c.Structure):
    _fields_ = [("words", c.c_uint64 * 20)]


def main():
    if len(sys.argv) != 2:
        raise SystemExit("usage: generate-slice-fixtures.py /absolute/path/to/built/libws_ffi")
    lib = c.CDLL(str(Path(sys.argv[1]).resolve()))
    lib.ws_abi_version.restype = c.c_uint32
    assert lib.ws_abi_version() == 6
    lib.ws_utc_to_sim.argtypes = [c.POINTER(Utc), c.POINTER(c.c_int64)]
    lib.ws_slice_create.argtypes, lib.ws_slice_create.restype = [c.POINTER(Config)], c.c_void_p
    lib.ws_slice_destroy.argtypes = [c.c_void_p]
    lib.ws_slice_apply.argtypes = [c.c_void_p, c.POINTER(Operation)]
    for name in ("ws_slice_fill_geometry", "ws_slice_fill_snapshot"):
        getattr(lib, name).argtypes = [c.c_void_p, c.c_void_p, c.c_size_t, c.POINTER(c.c_size_t)]
    lib.ws_slice_inspect.argtypes = [c.c_void_p, c.c_uint64, c.c_void_p, c.c_size_t, c.POINTER(c.c_size_t)]
    lib.ws_slice_get_calendar_sky.argtypes = [c.c_void_p, c.POINTER(Sky)]
    assert (c.sizeof(Utc), c.sizeof(Config), c.sizeof(Operation), c.sizeof(Sky), c.alignment(Sky)) == (40, 32, 64, 160, 8)
    utc = Utc(6, 40, 2026, 6, 22, 11, 22, 30, 0, 0)
    start = c.c_int64()
    assert lib.ws_utc_to_sim(c.byref(utc), c.byref(start)) == 0
    handle = lib.ws_slice_create(c.byref(Config(6, 32, 71449, start.value, 0)))
    assert handle

    def fill(name):
        n = c.c_size_t()
        assert getattr(lib, name)(handle, None, 0, c.byref(n)) == 3
        out = c.create_string_buffer(n.value)
        assert getattr(lib, name)(handle, out, len(out), c.byref(n)) == 0
        return out.raw[:n.value]

    try:
        geometry = fill("ws_slice_fill_geometry")
        camera = Operation(6, 64, 4, 0, (c.c_uint64 * 6)(0, 0, 0, struct.unpack("<Q", struct.pack("<d", 500.0))[0], 0, 0))
        assert lib.ws_slice_apply(handle, c.byref(camera)) == 0
        assert lib.ws_slice_apply(handle, c.byref(Operation(6, 64, 1, 0, (c.c_uint64 * 6)(20, 0, 0, 0, 0, 0)))) == 0
        snapshot = bytearray(fill("ws_slice_fill_snapshot"))
        sky = Sky()
        struct.pack_into("<II", sky, 0, 6, 160)
        assert lib.ws_slice_get_calendar_sky(handle, c.byref(sky)) == 0
        calendar = bytearray(c.string_at(c.byref(sky), 160))
        assert snapshot[8:32] == calendar[8:32]
        snapshot[24:32] = calendar[24:32] = bytes(8)  # Explicit Tier2 hash sentinel.
        fixture_dir = Path(__file__).parent / "fixtures"
        for label, offset in (("resident", 128), ("building", 128 + 30 * 48)):
            entity = struct.unpack_from("<Q", snapshot, offset)[0]
            out = c.create_string_buffer(8192)
            size = c.c_size_t()
            assert lib.ws_slice_inspect(handle, entity, out, len(out), c.byref(size)) == 0
            data = bytearray(out.raw[:size.value]); data[24:32] = bytes(8)
            (fixture_dir / ("inspect-" + label + "-v6.hex")).write_text(data.hex() + "\n")
            print("inspect-" + label, len(data), "bytes")
        for name, data in (("slice-geometry-v6", geometry), ("slice-snapshot-v6", snapshot), ("slice-sky-v6", calendar)):
            text = "\n".join(data[i:i+32].hex() for i in range(0, len(data), 32)) + "\n"
            (fixture_dir / (name + ".hex")).write_text(text)
            print(name, len(data), "bytes")
    finally:
        lib.ws_slice_destroy(handle)


if __name__ == "__main__":
    main()
