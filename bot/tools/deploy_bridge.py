from __future__ import annotations

import asyncio
import io
import ftplib
import json
import os
import secrets
import sys
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from signum_bot.bridge import connect_ftp  # noqa: E402
from signum_bot.config import BASE_DIR, Config  # noqa: E402
from signum_bot.customer_api import CustomerApi  # noqa: E402
from signum_bot.db import Db  # noqa: E402
from signum_bot.ipatool import Ipatool  # noqa: E402


def ensure_directory(client, path):
    import ftplib

    current = ""
    for part in path.strip("/").split("/"):
        current += "/" + part
        try:
            client.cwd(current)
        except ftplib.error_perm:
            client.mkd(current)
            client.cwd(current)


async def main():
    cfg = Config.load(require_bot_token=False)
    if cfg.public_base_url != "https://signum.appbank.pw":
        raise SystemExit("Set the production PUBLIC_BASE_URL before deployment")
    if not os.environ.get("BRIDGE_KEY"):
        key = secrets.token_hex(32)
        path = BASE_DIR / ".env"
        with path.open("a", encoding="utf-8") as stream:
            stream.write("\nBRIDGE_KEY=" + key + "\n")
        os.environ["BRIDGE_KEY"] = key
    cfg.prepare()
    db = Db(cfg.db_path)
    deps = SimpleNamespace(cfg=cfg, db=db, tool=Ipatool(cfg, db))
    api = CustomerApi(deps)
    assets = {}
    try:
        for name in ("docs", "panel", "schema"):
            response = await getattr(api, name)(None)
            assets[name + ".json"] = json.dumps(
                {
                    "body": response.text,
                    "headers": dict(response.headers),
                },
                ensure_ascii=False,
            ).encode()
    finally:
        db.conn.close()
    client = connect_ftp()
    try:
        private = os.environ.get("BRIDGE_STORAGE_DIR", "/signum-private")
        if private != "/signum-private":
            raise SystemExit("This deployment expects /signum-private storage")
        for folder in (
            private + "/files",
            private + "/queue",
            "/www/signum.appbank.pw",
        ):
            ensure_directory(client, folder)
        config = (
            "<?php\nreturn "
            + "['key'=>'"
            + os.environ["BRIDGE_KEY"]
            + "','base'=>'"
            + cfg.public_base_url
            + "'];\n"
        )
        existing = io.BytesIO()
        try:
            client.retrbinary("RETR " + private + "/config.php", existing.write)
        except ftplib.error_perm as error:
            if not str(error).startswith("550"):
                raise
        if existing.getvalue():
            source = existing.getvalue().decode("utf-8")
            if os.environ["BRIDGE_KEY"] not in source or cfg.public_base_url not in source:
                raise SystemExit("Existing bridge configuration differs; refusing to overwrite it")
        else:
            client.storbinary(
                "STOR " + private + "/config.php.part", io.BytesIO(config.encode())
            )
            client.rename(private + "/config.php.part", private + "/config.php")
        client.sendcmd("SITE CHMOD 700 " + private)
        client.sendcmd("SITE CHMOD 600 " + private + "/config.php")
        for name in ("index.php", ".htaccess"):
            assets[name] = (BASE_DIR / "deploy" / "bridge" / name).read_bytes()
        for name, body in assets.items():
            remote = "/www/signum.appbank.pw/" + name
            client.storbinary("STOR " + remote + ".part", io.BytesIO(body))
            client.rename(remote + ".part", remote)
        print("Bridge and API pages deployed through verified FTPS.")
        print(
            "Public directory: /www/signum.appbank.pw; private storage: /signum-private"
        )
    finally:
        client.close()


if __name__ == "__main__":
    asyncio.run(main())
