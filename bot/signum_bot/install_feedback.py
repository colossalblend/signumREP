from __future__ import annotations

import asyncio
import hashlib
import hmac
import json
import logging
import re
from pathlib import Path

from aiohttp import web
from aiogram import F, Router
from aiogram.filters import StateFilter
from aiogram.fsm.state import State, StatesGroup
from aiogram.utils.keyboard import InlineKeyboardBuilder

from . import ui
from .db import now

LOG = logging.getLogger("signum-bot.installation")
REASONS = (
    "iPhone пишет «Не удалось установить»",
    "Приложение не появилось на экране",
    "Не открывается ссылка",
    "Требуется другой Apple ID",
    "Загрузка остановилась",
)


class Feedback(StatesGroup):
    reason = State()


def record_event(db, event):
    token, identifier = event.get("token", ""), event.get("id", "")
    if not isinstance(token, str) or not re.fullmatch(r"[A-Za-z0-9_-]{16,64}", token):
        raise ValueError("invalid_token")
    if not isinstance(identifier, str) or not re.fullmatch(r"[a-f0-9]{32}", identifier):
        raise ValueError("invalid_event")
    if event.get("kind") != "iphone_download_finished":
        raise ValueError("invalid_event")
    row = db.file(token)
    if not row or row["expires_at"] <= now() or row["tg_id"] <= 0:
        return
    with db.conn:
        if not db.conn.execute(
            "insert or ignore into installation_events values (?,?)",
            (identifier, now()),
        ).rowcount:
            return
        db.conn.execute(
            "insert into installation_feedback(token,tg_id,due_at,created_at) values (?,?,?,?) "
            "on conflict(token) do update set due_at=excluded.due_at where status='pending'",
            (token, row["tg_id"], now() + 90, now()),
        )
        db.conn.execute(
            "delete from installation_events where created_at<?", (now() - 86400,)
        )


def register_events(app, cfg, db):
    async def receive(request):
        from .bridge import bridge_key

        if request.remote not in {"127.0.0.1", "::1"}:
            raise web.HTTPForbidden()
        body = await request.read()
        signature = hmac.new(bridge_key(), body, hashlib.sha256).hexdigest()
        if not hmac.compare_digest(
            signature, request.headers.get("X-Signum-Signature", "")
        ):
            raise web.HTTPForbidden()
        try:
            event = json.loads(body)
            if abs(now() - event["time"]) > 60:
                raise ValueError("expired_event")
            record_event(db, event)
        except (ValueError, KeyError, TypeError, AttributeError):
            raise web.HTTPBadRequest() from None
        return web.json_response({"ok": True})

    app.router.add_post("/internal/install-events", receive)


def question_kb(token):
    kb = InlineKeyboardBuilder()
    for text, answer in (
        ("Да", "yes"),
        ("Нет", "no"),
        ("Проблема с установкой", "issue"),
    ):
        kb.button(text=text, callback_data=f"ins:{token}:{answer}")
    kb.adjust(2, 1)
    return kb.as_markup()


async def prompt_loop(bot, cfg, db):
    db._run("update installation_feedback set status='issue' where status='reporting'")
    db._run(
        "update installation_feedback set status='pending',due_at=? where status='prompting'",
        (now() + 60,),
    )
    while True:
        from .production import maintenance_enabled

        if maintenance_enabled(db, "bot"):
            await asyncio.sleep(5)
            continue
        db._run(
            "delete from installation_feedback where created_at<?",
            (now() - 30 * 86400,),
        )
        rows = db.all(
            "select * from installation_feedback where status='pending' and due_at<=? limit 20",
            (now(),),
        )
        for item in rows:
            file = db.file(item["token"])
            if not file or file["expires_at"] <= now() or db.blocked(item["tg_id"]):
                db._run(
                    "update installation_feedback set status='expired' where token=?",
                    (item["token"],),
                )
                continue
            db._run(
                "update installation_feedback set status='prompting',attempts=attempts+1 where token=?",
                (item["token"],),
            )
            try:
                await bot.send_message(
                    item["tg_id"],
                    f"<b>{ui.esc(file['title'])}</b>\n\nВы установили приложение на iPhone?",
                    reply_markup=question_kb(item["token"]),
                )
            except Exception as exc:
                LOG.warning("Installation question failed (%s)", type(exc).__name__)
                db._run(
                    "update installation_feedback set status=?,due_at=? where token=?",
                    (
                        "pending" if item["attempts"] < 2 else "unreachable",
                        now() + 60,
                        item["token"],
                    ),
                )
            else:
                db._run(
                    "update installation_feedback set status='prompted' where token=?",
                    (item["token"],),
                )
        await asyncio.sleep(5)


async def remove_file(cfg, db, file):
    if file["remote"]:
        from .bridge import revoke_file

        await revoke_file(cfg, file["token"])
    path = Path(file["path"]).resolve()
    if path.is_relative_to(cfg.files_dir.resolve()):
        path.unlink(missing_ok=True)
        try:
            path.parent.rmdir()
        except OSError:
            pass
    db.file_delete(file["token"])


def build_feedback_router(cfg, db):
    router = Router()
    router.callback_query.filter(F.message.chat.type == "private")
    router.message.filter(F.chat.type == "private")

    def owned(token, actor):
        item = db.one(
            "select * from installation_feedback where token=? and tg_id=?",
            (token, actor),
        )
        file = db.file(token)
        if (
            not item
            or not file
            or file["tg_id"] != actor
            or db.blocked(actor)
            or item["status"] not in {"prompted", "issue"}
        ):
            return None
        return file

    async def report(bot, file, actor, reason):
        with db.conn:
            claimed = db.conn.execute(
                "update installation_feedback set status='reporting' where token=? and tg_id=? and status in ('prompted','issue')",
                (file["token"], actor),
            ).rowcount
        if not claimed:
            return False
        sent = False
        for admin in cfg.admins:
            try:
                await bot.send_message(
                    admin,
                    f'<b>Проблема с установкой</b>\n\nПриложение: <b>{ui.esc(file["title"])}</b>\nПользователь: <a href="tg://user?id={actor}">{actor}</a>\n\n<blockquote>{ui.esc(reason)}</blockquote>',
                )
                sent = True
            except Exception as exc:
                LOG.warning("Installation report failed (%s)", type(exc).__name__)
        if sent:
            db._run(
                "update installation_feedback set status='reported',reason=? where token=?",
                (reason, file["token"]),
            )
        else:
            db._run(
                "update installation_feedback set status='issue' where token=?",
                (file["token"],),
            )
        return sent

    @router.callback_query(F.data.startswith("ins:"))
    async def answer(cq, state):
        _, token, action = cq.data.split(":", 2)
        file = owned(token, cq.from_user.id)
        if not file:
            await cq.answer("Этот вопрос уже закрыт", show_alert=True)
            return
        await cq.answer()
        if action == "yes":
            from .anim import waiting

            try:
                await waiting(cq.message, remove_file(cfg, db, file), "Удаляю файл")
            except Exception as exc:
                LOG.warning(
                    "Confirmed installation cleanup failed (%s)", type(exc).__name__
                )
                await cq.message.answer(
                    "Не удалось удалить файл. Нажмите «Да» ещё раз."
                )
                return
            db._run(
                "update installation_feedback set status='installed' where token=?",
                (token,),
            )
            await cq.message.edit_text(
                "Приложение установлено. Файл удалён, ссылки больше не действуют."
            )
            if await state.get_state() == Feedback.reason.state:
                await state.clear()
        elif action == "no":
            db._run(
                "update installation_feedback set status='declined' where token=?",
                (token,),
            )
            await cq.message.edit_text(
                f"Вы можете установить позже.\nСсылка действует до {ui.when(file['expires_at'], with_time=True)}."
            )
        elif action == "issue":
            db._run(
                "update installation_feedback set status='issue' where token=?",
                (token,),
            )
            kb = InlineKeyboardBuilder()
            for index, reason in enumerate(REASONS):
                kb.button(text=reason, callback_data=f"inr:{token}:{index}")
            kb.button(text="Нету моей причины", callback_data=f"inr:{token}:other")
            kb.adjust(1)
            await cq.message.edit_text(
                "Что произошло при установке?", reply_markup=kb.as_markup()
            )

    @router.callback_query(F.data.startswith("inr:"))
    async def choose_reason(cq, state):
        _, token, choice = cq.data.split(":", 2)
        file = owned(token, cq.from_user.id)
        if not file:
            await cq.answer("Этот вопрос уже закрыт", show_alert=True)
            return
        await cq.answer()
        if choice == "other":
            await state.set_state(Feedback.reason)
            await state.update_data(installation_token=token)
            await cq.message.edit_text(
                "Опишите проблему одним сообщением.", reply_markup=ui.back_kb()
            )
        elif choice.isdigit() and int(choice) < len(REASONS):
            if await report(cq.bot, file, cq.from_user.id, REASONS[int(choice)]):
                await cq.message.edit_text("Сообщение отправлено администратору.")
            else:
                await cq.message.answer(
                    "Не удалось отправить сообщение. Попробуйте ещё раз."
                )

    @router.message(StateFilter(Feedback.reason), F.text)
    async def custom_reason(message, state):
        file = owned(
            (await state.get_data()).get("installation_token"), message.from_user.id
        )
        if not file:
            await state.clear()
            await message.answer("Этот вопрос уже закрыт.")
            return
        reason = message.text.strip()
        if not reason or len(reason) > 1500:
            await message.answer("Опишите проблему текстом до 1500 символов.")
            return
        if await report(message.bot, file, message.from_user.id, reason):
            await state.clear()
            await message.answer("Сообщение отправлено администратору.")
        else:
            await message.answer("Не удалось отправить сообщение. Попробуйте ещё раз.")

    return router
