                      
import struct
import sys

path = sys.argv[1] if len(sys.argv) > 1 else "/Applications/Signum.app/Contents/Resources/icon.icns"
with open(path, "rb") as f:
    data = f.read()
assert data[:4] == b"icns", data[:8]
size = struct.unpack(">I", data[4:8])[0]
print("file_size", len(data), "header_size", size)
off = 8
types = []
while off + 8 <= len(data):
    t = data[off : off + 4]
    s = struct.unpack(">I", data[off + 4 : off + 8])[0]
    if s < 8:
        break
    types.append((t.decode("latin1", "replace"), s))
    off += s
print("icons in icns:", len(types))
for t, s in types:
    print(f"  {t!r} {s} bytes")
