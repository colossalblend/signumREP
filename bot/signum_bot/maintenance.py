from __future__ import annotations

import asyncio
import logging

from cachetools import TTLCache

from .db import Db
from .ipatool import Ipatool

LOG = logging.getLogger("signum-bot.kbsync")


class KbsyncKeeper:
    def __init__(self, db: Db, tool: Ipatool) -> None:
        self.db = db
        self.tool = tool
        self.retry_after: TTLCache = TTLCache(maxsize=4096, ttl=900)

    async def tick(self) -> None:
        if not self.tool.is_cpp:
            return
        for account in self.db.kbsync_due():
            account_id = int(account["id"])
            if account_id in self.retry_after or self.tool.lock(account_id).locked():
                continue
            try:
                warmed = await self.tool.warm_kbsync(account)
            except Exception:
                self.retry_after[account_id] = True
                # CLI errors may contain credentials; log only the account id.
                LOG.warning(
                    "KBSync refresh failed for account %s; retry in 15 minutes",
                    account_id,
                )
            else:
                if warmed:
                    LOG.info(
                        "KBSync refreshed for account %s; next refresh in 12 hours",
                        account_id,
                    )

    async def run(self) -> None:
        while True:
            await self.tick()
            await asyncio.sleep(60)
