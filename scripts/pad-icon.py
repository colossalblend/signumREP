                      
"""Scale icon to SCALE of canvas and center on transparent square (for macOS Dock)."""
import sys
from pathlib import Path

try:
    from PIL import Image
except ImportError:
    import subprocess

    subprocess.check_call([sys.executable, "-m", "pip", "install", "--user", "pillow"])
    from PIL import Image

src = Path(sys.argv[1])
dst = Path(sys.argv[2])
canvas_size = int(sys.argv[3]) if len(sys.argv) > 3 else 1024
                                                                  
scale = float(sys.argv[4]) if len(sys.argv) > 4 else 0.78

img = Image.open(src).convert("RGBA")
inner = max(1, int(canvas_size * scale))
img = img.resize((inner, inner), Image.Resampling.LANCZOS)
canvas = Image.new("RGBA", (canvas_size, canvas_size), (0, 0, 0, 0))
off = (canvas_size - inner) // 2
canvas.paste(img, (off, off), img)
canvas.save(dst, format="PNG")
print(f"wrote {dst} {canvas_size}x{canvas_size} scale={scale}")
