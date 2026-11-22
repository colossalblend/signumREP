from __future__ import annotations

import asyncio
import json
import os
import time


async def heartbeat(cfg, service):
    path = cfg.data_dir / "logs" / f"{service}.health.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(".tmp")
    try:
        while True:
            temporary.write_text(
                json.dumps({"pid": os.getpid(), "updated_at": time.time()}),
                encoding="utf-8",
            )
            temporary.replace(path)
            await asyncio.sleep(15)
    finally:
        path.unlink(missing_ok=True)
        temporary.unlink(missing_ok=True)
