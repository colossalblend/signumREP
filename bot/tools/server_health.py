from __future__ import annotations

import json
import subprocess
import time
from pathlib import Path


def main():
    for name in ("api", "bot"):
        service = f"signum-{name}.service"
        result = subprocess.run(
            ["systemctl", "is-active", "--quiet", service], check=False
        )
        if result.returncode:
            continue
        path = Path("/opt/signum-bot/data/logs") / f"{name}.health.json"
        try:
            stamp = json.loads(path.read_text())["updated_at"]
        except (OSError, ValueError, KeyError, TypeError):
            stamp = 0
        if time.time() - stamp > 120:
            subprocess.run(["systemctl", "restart", service], check=True)


if __name__ == "__main__":
    main()
