from __future__ import annotations

import asyncio
import sys
import tempfile
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from signum_bot import ota
from signum_bot.api_keys import create_key
from signum_bot.commerce import key_digest
from signum_bot.config import Config
from signum_bot.customer_api import CustomerApi
from signum_bot.db import Db, now
from signum_bot.ipatool import Ipatool, IpatoolError
from signum_bot.store import Store


async def main():
    with tempfile.TemporaryDirectory() as directory:
        cfg = replace(
            Config.load(require_bot_token=False),
            token="",
            data_dir=Path(directory),
            http_host="127.0.0.1",
            http_port=8260,
            tls_port=0,
            payment_mode="demo",
            yookassa_secret_key="",
            public_base_url="http://127.0.0.1:8260",
        )
        cfg.prepare()
        db = Db(cfg.db_path)
        db.user(777, name="Проверка панели")
        db._run(
            "update users set api_until=?,api_remaining=1000,api_extra_remaining=240 where tg_id=777",
            (now() + 30 * 86400,),
        )
        identifier, _ = create_key(db, 777, "Основной", expires_at=now() + 30 * 86400)
        db._run(
            "update api_keys set digest=? where id=?",
            (key_digest("sg_" + "0" * 43), identifier),
        )
        tool = Ipatool(cfg, db)

        async def login(account, code=None):
            if account["password"] == "twofactor" and code != "123456":
                raise IpatoolError("Two-factor code required")

        tool.login = login
        store = Store(cfg)
        api = CustomerApi(SimpleNamespace(cfg=cfg, db=db, tool=tool, store=store))
        runner = await ota.serve(cfg, db, api)
        print("Local preview ready: http://127.0.0.1:8260/api/panel", flush=True)
        try:
            await asyncio.Event().wait()
        finally:
            await runner.cleanup()
            await store.close()
            db.conn.close()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        pass
