"""NBT, region files and gzip data files, read and written without loss.

Each tag reads as one of the classes below, so a value keeps its tag type and
writing it back gives the same bytes. Compounds keep their key order. Strings use
Java's modified UTF-8. Region chunks may be gzip, zlib or uncompressed, inline or in
an external `c.<x>.<z>.mcc` file; LZ4 chunks are not supported.
"""
import gzip
import json
import os
import re
import struct
import zlib


class Byte(int):
    tag = 1


class Short(int):
    tag = 2


class Int(int):
    tag = 3


class Long(int):
    tag = 4


class Float(float):
    tag = 5


class Double(float):
    tag = 6


class ByteArray(bytes):
    tag = 7


class String(str):
    tag = 8


class List(list):
    tag = 9

    def __init__(self, items=(), item_tag=0):
        super().__init__(items)
        self.item_tag = item_tag


class Compound(dict):
    tag = 10


class IntArray(list):
    tag = 11


class LongArray(list):
    tag = 12


SCALARS = {1: (Byte, ">b"), 2: (Short, ">h"), 3: (Int, ">i"), 4: (Long, ">q"), 5: (Float, ">f"), 6: (Double, ">d")}
ARRAYS = {7: (ByteArray, "b"), 11: (IntArray, "i"), 12: (LongArray, "q")}
SECTOR = 4096
COMPRESSION_GZIP, COMPRESSION_ZLIB, COMPRESSION_NONE, COMPRESSION_LZ4 = 1, 2, 3, 4
EXTERNAL = 0x80
BARE_KEY = re.compile(r"[A-Za-z0-9._+-]+")


def _decode_string(raw):
    return bytes(raw).replace(b"\xc0\x80", b"\0").decode("utf-8", "surrogatepass")


def _encode_string(text):
    if any(ord(c) > 0xFFFF for c in text):
        # Java writes characters outside the BMP as two 3-byte surrogates (CESU-8).
        text = text.encode("utf-16-be", "surrogatepass")
        text = "".join(chr(int.from_bytes(text[i:i + 2], "big")) for i in range(0, len(text), 2))
    return text.encode("utf-8", "surrogatepass").replace(b"\0", b"\xc0\x80")


def _read(buf, i, tag):
    if tag in SCALARS:
        cls, fmt = SCALARS[tag]
        return cls(struct.unpack_from(fmt, buf, i)[0]), i + struct.calcsize(fmt)
    if tag in ARRAYS:
        cls, fmt = ARRAYS[tag]
        n = struct.unpack_from(">i", buf, i)[0]
        i += 4
        if cls is ByteArray:
            return ByteArray(buf[i:i + n]), i + n
        return cls(struct.unpack_from(f">{n}{fmt}", buf, i)), i + n * struct.calcsize(fmt)
    if tag == 8:
        n = struct.unpack_from(">H", buf, i)[0]
        return String(_decode_string(buf[i + 2:i + 2 + n])), i + 2 + n
    if tag == 9:
        item_tag, n = struct.unpack_from(">bi", buf, i)
        i += 5
        items = List(item_tag=item_tag)
        for _ in range(n):
            value, i = _read(buf, i, item_tag)
            items.append(value)
        return items, i
    if tag == 10:
        compound = Compound()
        while True:
            child = buf[i]
            i += 1
            if child == 0:
                return compound, i
            n = struct.unpack_from(">H", buf, i)[0]
            name = _decode_string(buf[i + 2:i + 2 + n])
            compound[name], i = _read(buf, i + 2 + n, child)
    raise ValueError(f"unknown tag type {tag} at byte {i}")


def _write(out, value):
    tag = value.tag
    if tag in SCALARS:
        out.append(struct.pack(SCALARS[tag][1], value))
    elif tag in ARRAYS:
        out.append(struct.pack(">i", len(value)))
        out.append(bytes(value) if tag == 7 else struct.pack(f">{len(value)}{ARRAYS[tag][1]}", *value))
    elif tag == 8:
        raw = _encode_string(value)
        out.append(struct.pack(">H", len(raw)) + raw)
    elif tag == 9:
        out.append(struct.pack(">bi", value[0].tag if value else value.item_tag, len(value)))
        for item in value:
            _write(out, item)
    else:
        for name, child in value.items():
            raw = _encode_string(name)
            out.append(struct.pack(">bH", child.tag, len(raw)) + raw)
            _write(out, child)
        out.append(b"\0")


def _float_text(value, fmt):
    """The shortest decimal text that reads back as the same float or double."""
    for digits in range(1, 18):
        text = f"{value:.{digits}g}"
        if struct.pack(fmt, float(text)) == struct.pack(fmt, value):
            return text
    return repr(float(value))


def snbt_key(name):
    return name if BARE_KEY.fullmatch(name) else json.dumps(name, ensure_ascii=False)


def to_snbt(value):
    """SNBT text as /data get prints it; plain JSON values print as JSON."""
    tag = getattr(value, "tag", None)
    if tag in (1, 2, 4):
        return f"{int(value)}{'bsL'[(1, 2, 4).index(tag)]}"
    if tag == 3:
        return str(int(value))
    if tag in (5, 6):
        return _float_text(value, ">f" if tag == 5 else ">d") + ("f" if tag == 5 else "d")
    if tag == 7:
        return "[B;" + ",".join(f"{b}b" for b in struct.unpack(f">{len(value)}b", value)) + "]"
    if tag in (11, 12):
        return ("[I;" if tag == 11 else "[L;") + ",".join(f"{v}{'L' if tag == 12 else ''}" for v in value) + "]"
    if isinstance(value, dict):
        return "{" + ",".join(f"{snbt_key(k)}:{to_snbt(v)}" for k, v in value.items()) + "}"
    if isinstance(value, list):
        return "[" + ",".join(to_snbt(v) for v in value) + "]"
    return json.dumps(value, ensure_ascii=False)


def loads(data):
    """Reads one named root tag. Returns (name, value)."""
    tag = data[0]
    n = struct.unpack_from(">H", data, 1)[0]
    value, end = _read(data, 3 + n, tag)
    if end != len(data):
        raise ValueError(f"{len(data) - end} bytes after the root tag")
    return _decode_string(data[3:3 + n]), value


def dumps(value, name=""):
    raw = _encode_string(name)
    out = [struct.pack(">bH", value.tag, len(raw)) + raw]
    _write(out, value)
    return b"".join(out)


def read_file(path):
    """Reads a data file such as level.dat, gzip or not. Returns (name, value)."""
    with open(path, "rb") as f:
        data = f.read()
    return loads(gzip.decompress(data) if data[:2] == b"\x1f\x8b" else data)


def write_file(path, value, name=""):
    with open(path, "wb") as f:
        f.write(gzip.compress(dumps(value, name), mtime=0))


def region_position(path):
    """(region x, region z) from a file name like r.-1.2.mca."""
    _, x, z, _ = os.path.basename(path).split(".")
    return int(x), int(z)


def _decompress(kind, data):
    if kind == COMPRESSION_GZIP:
        return gzip.decompress(data)
    if kind == COMPRESSION_ZLIB:
        return zlib.decompress(data)
    if kind == COMPRESSION_NONE:
        return data
    raise ValueError(f"chunk compression {kind} is not supported")


def region_chunks(path):
    """Yields (chunk x, chunk z, timestamp, uncompressed NBT bytes) for each chunk in a region file."""
    rx, rz = region_position(path)
    with open(path, "rb") as f:
        data = f.read()
    if not data:
        return
    for index in range(1024):
        location = int.from_bytes(data[index * 4:index * 4 + 4], "big")
        if not location:
            continue
        offset = (location >> 8) * SECTOR
        timestamp = int.from_bytes(data[SECTOR + index * 4:SECTOR + index * 4 + 4], "big")
        length, kind = struct.unpack_from(">IB", data, offset)
        cx, cz = rx * 32 + index % 32, rz * 32 + index // 32
        if kind & EXTERNAL:
            with open(os.path.join(os.path.dirname(path), f"c.{cx}.{cz}.mcc"), "rb") as f:
                payload = f.read()
        else:
            payload = data[offset + 5:offset + 4 + length]
        yield cx, cz, timestamp, _decompress(kind & ~EXTERNAL, payload)


def read_region(path):
    """Returns {(chunk x, chunk z): (timestamp, Compound)}."""
    return {(cx, cz): (timestamp, loads(raw)[1]) for cx, cz, timestamp, raw in region_chunks(path)}


def write_region(path, chunks):
    """Writes {(chunk x, chunk z): (timestamp, Compound)} as a zlib region file, like vanilla's default."""
    rx, rz = region_position(path)
    header, timestamps, body = bytearray(SECTOR), bytearray(SECTOR), bytearray()
    for (cx, cz), (timestamp, value) in sorted(chunks.items()):
        index = (cx - rx * 32) + (cz - rz * 32) * 32
        if not 0 <= index < 1024:
            raise ValueError(f"chunk {cx},{cz} is not in {os.path.basename(path)}")
        payload = zlib.compress(dumps(value))
        kind = COMPRESSION_ZLIB
        if len(payload) + 5 > 255 * SECTOR:
            with open(os.path.join(os.path.dirname(path), f"c.{cx}.{cz}.mcc"), "wb") as f:
                f.write(payload)
            payload, kind = b"", kind | EXTERNAL
        record = struct.pack(">IB", len(payload) + 1, kind) + payload
        record += b"\0" * (-len(record) % SECTOR)
        sector = 2 + len(body) // SECTOR
        header[index * 4:index * 4 + 4] = (sector << 8 | len(record) // SECTOR).to_bytes(4, "big")
        timestamps[index * 4:index * 4 + 4] = timestamp.to_bytes(4, "big")
        body += record
    with open(path, "wb") as f:
        f.write(header + timestamps + body)
