from __future__ import annotations

import asyncio
import logging
from contextlib import suppress
from types import SimpleNamespace

from . import ota
from .commerce import upgrade_api_packages
from .config import Config
from .customer_api import CustomerApi
from .db import Db
from .ipatool import Ipatool
from .payments import Payments
from .store import Store
from .production import configure_logs, backup_loop
from .runtime_health import heartbeat


async def main():
    cfg = Config.load(require_bot_token=False)
    cfg.prepare()
    configure_logs(cfg, "api")
    db = Db(cfg.db_path)
    upgrade_api_packages(cfg, db)
    deps = SimpleNamespace(
        cfg=cfg,
        db=db,
        tool=Ipatool(cfg, db),
        store=Store(cfg),
        payments=Payments(cfg, db),
    )
    runner = await ota.serve(cfg, db, CustomerApi(deps))
    cleaner = asyncio.create_task(ota.cleaner(cfg, db))
    health = asyncio.create_task(heartbeat(cfg, "api"))
    backups = asyncio.create_task(backup_loop(cfg))
    bridge = None
    if cfg.bridge_enabled:
        from .bridge import BridgeWorker

        bridge = asyncio.create_task(BridgeWorker(cfg).run())
    try:
        await asyncio.Event().wait()
    finally:
        tasks = [cleaner, health, backups] + ([bridge] if bridge else [])
        for task in tasks:
            task.cancel()
        for task in tasks:
            with suppress(asyncio.CancelledError):
                await task
        await runner.cleanup()
        await deps.store.close()
        db.conn.close()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        pass
    except Exception:
        logging.exception("API startup failed")
        raise SystemExit(1)
