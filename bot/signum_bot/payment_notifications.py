from __future__ import annotations

import asyncio
import html
import logging
from decimal import Decimal

from aiogram.exceptions import TelegramForbiddenError, TelegramRetryAfter

from .db import now
from .ui import PLAN_TITLES

LOG = logging.getLogger("signum-bot.payments")


def notification_text(row):
    name = html.escape(row["name"] or "Пользователь")
    username = html.escape(row["username"] or "")
    profile = f'<a href="tg://user?id={row["tg_id"]}">{name}</a>'
    if username:
        profile += f" · @{username}"
    value = Decimal(row["amount"]) / 100
    amount = f"{value:.0f}" if value == value.to_integral_value() else f"{value:.2f}"
    plan = html.escape(PLAN_TITLES.get(row["plan"], row["plan"]))
    return (
        "✅ <b>Успешная оплата</b>\n\n"
        f"{profile}\nTelegram ID: <code>{row['tg_id']}</code>\n\n"
        f"<b>{plan}</b>\nСумма: <b>{amount} ₽</b>\n"
        f"Платёж: <code>{html.escape(row['charge_id'])}</code>"
    )


async def deliver_pending(bot, cfg, db):
    rows = db.all(
        "select n.*,p.tg_id,p.plan,p.amount,u.name,u.username "
        "from payment_notifications n join payments p on p.charge_id=n.charge_id "
        "left join users u on u.tg_id=p.tg_id "
        "where n.delivered_at=0 and n.next_attempt<=? and p.mode='real' "
        "order by n.created_at limit 20", (now(),)
    )
    for row in rows:
        recipient = row["admin_id"]
        if recipient not in cfg.admins:
            db._run(
                "update payment_notifications set delivered_at=-2 where charge_id=? and admin_id=?",
                (row["charge_id"], recipient),
            )
            continue
        identity = (row["charge_id"], recipient)
        claimed = db._run(
            "update payment_notifications set next_attempt=?,attempts=attempts+1 "
            "where charge_id=? and admin_id=? and delivered_at=0 and next_attempt<=?",
            (now() + 90, *identity, now()),
        )
        if not claimed.rowcount:
            continue
        try:
            await bot.send_message(recipient, notification_text(row), parse_mode="HTML")
        except TelegramForbiddenError:
            db._run(
                "update payment_notifications set delivered_at=-1 where charge_id=? and admin_id=?",
                identity,
            )
            LOG.warning("Payment notification blocked by administrator")
        except Exception as exc:
            delay = min(600, 15 * 2 ** min(row["attempts"], 6))
            if isinstance(exc, TelegramRetryAfter):
                delay = max(delay, exc.retry_after)
            db._run(
                "update payment_notifications set next_attempt=? where charge_id=? and admin_id=?",
                (now() + delay, *identity),
            )
            LOG.warning("Payment notification deferred (%s)", type(exc).__name__)
        else:
            db._run(
                "update payment_notifications set delivered_at=? where charge_id=? and admin_id=?",
                (now(), *identity),
            )


async def notification_loop(bot, cfg, db):
    while True:
        try:
            await deliver_pending(bot, cfg, db)
        except Exception as exc:
            LOG.warning("Payment notification queue deferred (%s)", type(exc).__name__)
        await asyncio.sleep(5)
