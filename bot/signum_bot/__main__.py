from __future__ import annotations

import asyncio
import logging
import os
from contextlib import suppress

from aiogram import Bot, Dispatcher
from aiogram.client.default import DefaultBotProperties
from aiogram.client.session.aiohttp import AiohttpSession
from aiogram.client.telegram import TelegramAPIServer
from aiogram.enums import ParseMode
from aiogram.types import BotCommand

from . import ota
from .cards import Cards
from .config import Config
from .db import Db
from .customer_api import CustomerApi
from .commerce import upgrade_api_packages
from .handlers import Deps, build_router
from .ipatool import Ipatool
from .sender import FileSender
from .store import Store
from .maintenance import KbsyncKeeper
from .theme import Theme
from .payments import Payments

COMMANDS = [
    BotCommand(command="start", description="Главное меню"),
    BotCommand(command="buy", description="Подписка и лимиты"),
    BotCommand(command="help", description="Как это работает"),
    BotCommand(command="apple", description="Мой Apple ID"),
    BotCommand(command="purchases", description="История покупок Apple ID"),
    BotCommand(command="id", description="Мой Telegram ID"),
    BotCommand(command="api", description="Signum API"),
]


async def main() -> None:
    from .api_lease import ApiLease
    from .production import BotGuard, configure_logs
    from .runtime_health import heartbeat

    cfg = Config.load()
    cfg.prepare()
    configure_logs(cfg, "bot")
    log = logging.getLogger("signum-bot")
    lease = ApiLease(cfg.data_dir / "bot-server.lock", "Бот уже запущен.")
    db = Db(cfg.db_path)
    db._run("delete from storage_reservations where id like 'bot:%'")
    db._run("delete from download_claims")
    upgrade_api_packages(cfg, db)
    ota.migrate_links(cfg, db)
    sender = FileSender(cfg)
    deps = Deps(
        cfg=cfg,
        db=db,
        store=Store(cfg),
        tool=Ipatool(cfg, db),
        sender=sender,
        cards=Cards(cfg),
        payments=Payments(cfg, db),
    )
    session = (
        AiohttpSession(
            api=TelegramAPIServer.from_base(cfg.local_api_url, is_local=True)
        )
        if cfg.local_api_url
        else None
    )
    bot = Bot(
        cfg.token,
        session=session,
        default=DefaultBotProperties(parse_mode=ParseMode.HTML),
    )
    runner = None
    tasks = []

    async def warm_sender():
        try:
            await sender.start()
            log.info("File sender connected")
        except Exception as exc:
            log.warning(
                "File sender unavailable (%s); polling stays active", type(exc).__name__
            )

    try:
        dp = Dispatcher()
        guard = BotGuard(db, cfg.admins)
        dp.message.outer_middleware(guard)
        dp.callback_query.outer_middleware(guard)
        try:
            async with asyncio.timeout(15):
                theme = await Theme.load(bot)
        except TimeoutError:
            theme = Theme({})
        bot.session.middleware(theme)
        from .install_feedback import build_feedback_router, prompt_loop

        dp.include_router(build_feedback_router(cfg, db))
        dp.include_router(build_router(deps))
        tasks.append(asyncio.create_task(prompt_loop(bot, cfg, db)))
        from .payment_notifications import notification_loop

        tasks.append(asyncio.create_task(notification_loop(bot, cfg, db)))
        if sender.ready:
            tasks.append(asyncio.create_task(warm_sender()))
        if cfg.api_embedded:
            in_process = cfg.bridge_enabled and os.getenv("BRIDGE_IN_PROCESS") == "1"
            runner = await ota.serve(cfg, db, CustomerApi(deps), in_process=in_process)
            tasks.append(asyncio.create_task(ota.cleaner(cfg, db)))
            tasks.append(asyncio.create_task(heartbeat(cfg, "api")))
            from .production import backup_loop
            tasks.append(asyncio.create_task(backup_loop(cfg)))
            if cfg.bridge_enabled:
                from .bridge import BridgeWorker
                tasks.append(asyncio.create_task(BridgeWorker(cfg, runner.app if in_process else None).run()))
        tasks.append(asyncio.create_task(KbsyncKeeper(db, deps.tool).run()))
        tasks.append(asyncio.create_task(heartbeat(cfg, "bot")))
        try:
            async with asyncio.timeout(10):
                await bot.set_my_commands(COMMANDS)
        except Exception as exc:
            log.warning("Command registration deferred (%s)", type(exc).__name__)
        await dp.start_polling(bot, tasks_concurrency_limit=64)
    finally:
        for task in tuple(deps.job_tasks):
            task.cancel()
        if deps.job_tasks:
            await asyncio.gather(*deps.job_tasks, return_exceptions=True)
        for task in tasks:
            task.cancel()
        for task in tasks:
            with suppress(asyncio.CancelledError):
                await task
        await sender.close()
        if deps.apple:
            await deps.apple.close()
        if runner:
            await runner.cleanup()
        else:
            await deps.payments.close()
        await deps.store.close()
        await bot.session.close()
        db.conn.close()
        lease.close()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        pass
    except Exception:
        logging.exception("Bot startup failed")
        raise SystemExit(1)
