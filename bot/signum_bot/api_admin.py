from __future__ import annotations

import argparse
import json
import secrets

from .api_keys import create_key
from .config import Config
from .db import Db, now


def main():
    parser = argparse.ArgumentParser(description="Создать API-клиента без Telegram")
    parser.add_argument("--name", required=True)
    parser.add_argument("--days", type=int, default=30)
    parser.add_argument("--downloads", type=int, default=1000)
    args = parser.parse_args()
    if (
        not 1 <= args.days <= 365
        or not 0 <= args.downloads <= 1_000_000
        or not 1 <= len(args.name.strip()) <= 40
    ):
        parser.error("Проверьте название, срок и количество загрузок")
    cfg = Config.load(require_bot_token=False)
    cfg.prepare()
    db = Db(cfg.db_path)
    owner = secrets.randbits(52) + 10**15
    try:
        db.user(owner, name=args.name.strip())
        until = now() + args.days * 86400
        db._run(
            "update users set api_until=?,api_remaining=?,api_only=1 where tg_id=?",
            (until, args.downloads, owner),
        )
        identifier, key = create_key(db, owner, "Основной", expires_at=until)
        print(
            json.dumps(
                {
                    "client_id": owner,
                    "key_id": identifier,
                    "api_key": key,
                    "expires_at": until,
                },
                ensure_ascii=False,
            )
        )
    finally:
        db.conn.close()


if __name__ == "__main__":
    main()
