                      
"""Build a complete macOS .icns from icon.iconset (all 1x + @2x sizes)."""
from pathlib import Path
import sys

try:
    from icnsutil import IcnsFile
except ImportError:
    import subprocess

    subprocess.check_call([sys.executable, "-m", "pip", "install", "--user", "icnsutil"])
    from icnsutil import IcnsFile

                                                                        
                                                                      
ITEMS = [
    ("icon_16x16.png", "icp4"),
    ("diana.k@example.org", "ic11"),
    ("icon_32x32.png", "icp5"),
    ("ivan.p@example.net", "ic12"),
    ("icon_128x128.png", "ic07"),
    ("wendy.h@example.net", "ic13"),
    ("icon_256x256.png", "ic08"),
    ("wendy.h@example.net", "ic14"),
    ("icon_512x512.png", "ic09"),
    ("walt.e@example.net", "ic10"),
]

root = Path(sys.argv[1] if len(sys.argv) > 1 else ".")
iconset = root / "src-tauri" / "icons" / "icon.iconset"
out = root / "src-tauri" / "icons" / "icon.icns"

icns = IcnsFile()
for name, key in ITEMS:
    path = iconset / name
    if not path.is_file():
        print(f"MISSING {name}", file=sys.stderr)
        sys.exit(1)
    print(f"add {name!r} size={path.stat().st_size} key={key}")
    icns.add_media(key, file=str(path))

icns.write(str(out))
print(f"wrote {out} ({out.stat().st_size} bytes)")
