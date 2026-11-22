from __future__ import annotations

import re
import secrets

from aiogram import F, Router
from aiogram.filters import Command, StateFilter
from aiogram.fsm.state import State, StatesGroup
from aiogram.utils.keyboard import InlineKeyboardBuilder

from . import ui
from .anim import waiting
from .apple_web import AppleWebError
from .db import now


class AppleAction(StatesGroup):
    busy = State()
    code = State()
    address = State()
    confirm = State()


ERRORS = {
    "wrong_password": "Пароль Apple ID не подошёл. Войдите в аккаунт заново.",
    "account_locked": "Apple ID временно заблокирован. Проверьте его на сайте Apple.",
    "active_subscriptions": "Apple не разрешает смену страны из-за действующей подписки. Отмените её и дождитесь окончания оплаченного периода.",
    "remaining_balance": "Сначала потратьте остаток на балансе Apple ID, затем повторите смену страны.",
    "family_sharing": "Сначала выйдите из семейной группы Apple, затем повторите смену страны.",
    "payment_required": "Для этой страны Apple требует способ оплаты. Завершите смену страны на сайте Apple.",
    "region_unavailable": "Apple не разрешил смену страны. Проверьте баланс и подписки аккаунта или завершите смену на сайте Apple.",
}


def buttons(rows):
    kb = InlineKeyboardBuilder()
    for label, callback in rows:
        kb.button(text=label, callback_data=callback)
    kb.adjust(1)
    return kb.as_markup()


def history_view(result):
    page = result["page"]
    lines = [f"🧾 <b>История покупок</b>\n{ui.LINE}"]
    kb = InlineKeyboardBuilder()
    for number, app in enumerate(result["apps"], (page - 1) * 8 + 1):
        name = app.get("name") or "Приложение"
        detail = " · ".join(
            str(value) for value in (app.get("date"), app.get("price")) if value
        )
        lines.append(
            f"<b>{number}. {ui.esc(name[:100])}</b>"
            + (f"\n<i>{ui.esc(detail[:100])}</i>" if detail else "")
        )
        if re.fullmatch(r"[0-9]{1,20}", str(app["id"])):
            kb.button(text=f"{number}. {name[:48]}", callback_data=f"app:{app['id']}")
    if not result["apps"]:
        lines.append(
            "Покупок приложений пока нет."
            if page == 1
            else "Вы просмотрели все покупки."
        )
    kb.adjust(1)
    nav = InlineKeyboardBuilder()
    if page > 1:
        nav.button(text="◀︎ Назад", callback_data=f"pur:{page - 1}")
    if result["has_more"]:
        next_page = result.get("next_page", page + 1)
        nav.button(
            text="Показать ещё" if next_page == page else "Вперёд ▶︎",
            callback_data=f"pur:{next_page}",
        )
    nav.adjust(2)
    kb.attach(nav)
    kb.attach(
        InlineKeyboardBuilder.from_markup(
            buttons([("Обновить покупки", "pur:refresh"), ("Мой Apple ID", "me")])
        )
    )
    lines.append(
        f"{ui.THIN}\nСтраница <b>{page}</b>\n<i>Нажмите на приложение, чтобы открыть его.</i>"
    )
    return "\n\n".join(lines), kb.as_markup()


def build_apple_screens(deps, edit):
    router = Router()
    router.callback_query.filter(F.message.chat.type == "private")
    router.message.filter(F.chat.type == "private")
    db, tool, apple = deps.db, deps.tool, deps.apple

    def account_for(actor, identifier=None):
        account = db.account(identifier) if identifier else db.account_of(actor)
        if account is None or account["tg_id"] != actor or db.blocked(actor):
            return None
        return account

    async def fail(message, text):
        await edit(
            message,
            f"<b>Не удалось выполнить запрос</b>\n{ui.LINE}\n{ui.esc(text)}",
            reply_markup=buttons(
                [("Мой Apple ID", "me"), ("Открыть сайт Apple", "awsupport")]
            ),
        )

    async def operation(message, state, account, action, **fields):
        token = secrets.token_urlsafe(8)
        prior = await state.get_data()
        await state.set_state(AppleAction.busy)
        await state.update_data(operation_token=token)
        try:

            async def perform():
                if action == "history":
                    return await apple.history(account, **fields)
                if action == "region_change":
                    async with tool.lock(account["id"]):
                        return await apple.request(account, action, **fields)
                return await apple.request(account, action, **fields)

            request = perform()
            result = await waiting(
                message,
                request,
                "Открываю покупки"
                if action == "history"
                else "Обновляю страну App Store"
                if action == "region_change"
                else "Проверяю страну App Store",
            )
        except (AppleWebError, TimeoutError):
            result = {
                "status": "failed",
                "error": "apple_unavailable",
            }
        if (await state.get_data()).get("operation_token") != token:
            return
        if result["status"] == "requires_2fa":
            attempts = prior.get("attempts", 0) + (1 if fields.get("code") else 0)
            if attempts >= 3:
                await apple.stop_worker(account["id"])
                await state.clear()
                await fail(message, "Код трижды не подошёл. Начните запрос заново.")
                return
            await state.set_state(AppleAction.code)
            retry_fields = {
                key: value for key, value in fields.items() if key != "code"
            }
            if action == "history":
                retry_fields["refresh"] = False
            await state.update_data(
                account_id=account["id"],
                action=action,
                fields=retry_fields,
                attempts=attempts,
                expires_at=prior.get("expires_at") or now() + 600,
            )
            prefix = (
                "Код не подошёл. "
                if fields.get("code")
                else "Apple запросил подтверждение. "
            )
            await edit(
                message,
                f"<b>Введите код Apple</b>\n{ui.LINE}\n{prefix}Пришлите <b>6 цифр</b> с устройства Apple или из SMS.",
                reply_markup=buttons([("Отменить", "awcancel")]),
            )
            return
        await state.clear()
        if result["status"] != "ready":
            await fail(
                message,
                ERRORS.get(
                    result.get("error"), "Apple временно недоступен. Попробуйте позже."
                ),
            )
            return
        if action == "history":
            for app in result["apps"]:
                if re.fullmatch(r"[0-9]{1,20}", str(app.get("id", ""))):
                    cached = db.app(str(app["id"]))
                    db.app_save(
                        {
                            **(dict(cached) if cached else {}),
                            "adam_id": str(app["id"]),
                            "title": app.get("name") or str(app["id"]),
                            "artist": app.get("artist") or "",
                            "icon": app.get("artwork") or "",
                        }
                    )
            text, markup = history_view(result)
            await edit(message, text, reply_markup=markup)
        else:
            region = result["region"]
            name = region.get("countryName") or region.get("countryCode")
            title = (
                "Страна App Store изменена"
                if action == "region_change"
                else "Страна App Store"
            )
            await edit(
                message,
                f"<b>{title}</b>\n{ui.LINE}\n<b>{ui.esc(name)}</b>",
                reply_markup=buttons(
                    [("Сменить страну", "awcountries:0"), ("Мой Apple ID", "me")]
                ),
            )

    @router.callback_query(F.data.startswith("pur:"))
    async def purchases(cq, state):
        account = account_for(cq.from_user.id)
        if not account:
            await cq.answer("Сначала подключите Apple ID", show_alert=True)
            return
        if await state.get_state() in (AppleAction.busy.state, AppleAction.code.state):
            await cq.answer(
                "Завершите текущий запрос или отмените его", show_alert=True
            )
            return
        value = cq.data.split(":", 1)[1]
        if value != "refresh" and (not value.isdigit() or not 1 <= int(value) <= 1000):
            await cq.answer()
            return
        await cq.answer()
        await state.clear()
        await operation(
            cq.message,
            state,
            account,
            "history",
            page=1 if value == "refresh" else int(value),
            refresh=value == "refresh",
        )

    @router.message(Command("purchases"))
    async def purchase_command(message, state):
        account = account_for(message.from_user.id)
        if not account:
            await message.answer(
                "Сначала подключите Apple ID.",
                reply_markup=ui.back_kb("me", "Мой Apple ID"),
            )
            return
        if await state.get_state() in (AppleAction.busy.state, AppleAction.code.state):
            await message.answer("Завершите текущий запрос или отмените его.")
            return
        note = await message.answer("<b>История покупок</b>")
        await state.clear()
        await operation(note, state, account, "history", page=1)

    @router.callback_query(F.data == "awregion")
    async def current_region(cq, state):
        account = account_for(cq.from_user.id)
        if not account:
            await cq.answer("Сначала подключите Apple ID", show_alert=True)
            return
        if await state.get_state() in (AppleAction.busy.state, AppleAction.code.state):
            await cq.answer(
                "Завершите текущий запрос или отмените его", show_alert=True
            )
            return
        await cq.answer()
        await state.clear()
        await operation(cq.message, state, account, "region_status")

    @router.callback_query(F.data.startswith("awcountries:"))
    async def country_list(cq, state):
        if not account_for(cq.from_user.id):
            await cq.answer("Сначала подключите Apple ID", show_alert=True)
            return
        if await state.get_state() in (AppleAction.busy.state, AppleAction.code.state):
            await cq.answer(
                "Завершите текущий запрос или отмените его", show_alert=True
            )
            return
        raw = cq.data.split(":", 1)[1]
        if not raw.isdigit() or int(raw) > 100:
            await cq.answer()
            return
        await cq.answer()
        await state.clear()
        try:
            countries = await apple.regions()
        except AppleWebError:
            await fail(cq.message, "Не удалось загрузить страны. Попробуйте позже.")
            return
        page = int(raw)
        kb = InlineKeyboardBuilder()
        for country in countries[page * 12 : (page + 1) * 12]:
            kb.button(text=country["name"], callback_data=f"awc:{country['code']}")
        kb.adjust(2)
        nav = []
        if page:
            nav.append(("◀︎ Назад", f"awcountries:{page - 1}"))
        if (page + 1) * 12 < len(countries):
            nav.append(("Вперёд ▶︎", f"awcountries:{page + 1}"))
        kb.attach(
            InlineKeyboardBuilder.from_markup(buttons(nav + [("Мой Apple ID", "me")]))
        )
        await edit(
            cq.message,
            f"<b>Сменить страну App Store</b>\n{ui.LINE}\nВыберите новую страну.\n<blockquote>Apple может отказать, если остались деньги на балансе или действующие подписки.</blockquote>",
            reply_markup=kb.as_markup(),
        )

    async def confirmation(message, state, account, country, address):
        token = secrets.token_urlsafe(8)
        await state.set_state(AppleAction.confirm)
        await state.update_data(
            account_id=account["id"],
            country=country,
            address=address,
            confirm_token=token,
            expires_at=now() + 600,
        )
        address_text = ", ".join(
            str(address.get(key) or "")
            for key in ("street", "city", "postalCode", "state", "county")
            if address.get(key)
        )
        await edit(
            message,
            f"<b>Подтвердите смену страны</b>\n{ui.LINE}\nПочта: <code>{ui.esc(account['email'])}</code>\nНовая страна: <b>{ui.esc(address['countryName'])}</b>\n\nАдрес для App Store:\n<blockquote>{ui.esc(address_text)}\nТелефон: {ui.esc(address.get('phoneArea', '') + ' ' + address.get('phoneNumber', ''))}</blockquote>\n<i>Проверьте адрес перед подтверждением. При необходимости введите свой.</i>",
            reply_markup=buttons(
                [
                    ("Сменить страну", f"awapply:{token}"),
                    ("Ввести свой адрес", "awaddress"),
                    ("Отмена", "awcancel"),
                ]
            ),
        )

    @router.callback_query(F.data.startswith("awc:"))
    async def choose_country(cq, state):
        account = account_for(cq.from_user.id)
        if not account:
            await cq.answer("Сначала подключите Apple ID", show_alert=True)
            return
        if await state.get_state() in (AppleAction.busy.state, AppleAction.code.state):
            await cq.answer(
                "Завершите текущий запрос или отмените его", show_alert=True
            )
            return
        country = cq.data.split(":", 1)[1]
        try:
            options = await apple.regions()
            if country not in {row["code"] for row in options}:
                await cq.answer("Выберите страну из списка", show_alert=True)
                return
            await cq.answer()
            result = await apple.request(None, "addresses", country=country)
            if not result.get("addresses"):
                await fail(
                    cq.message, "Для этой страны смена через бота пока недоступна."
                )
                return
            await state.clear()
            await confirmation(
                cq.message, state, account, country, result["addresses"][0]
            )
        except AppleWebError:
            await fail(
                cq.message, "Не удалось загрузить данные страны. Попробуйте позже."
            )

    @router.callback_query(F.data.startswith("awapply:"))
    async def apply_country(cq, state):
        data = await state.get_data()
        account = account_for(cq.from_user.id, data.get("account_id"))
        if (
            not account
            or await state.get_state() != AppleAction.confirm.state
            or data.get("expires_at", 0) <= now()
            or not secrets.compare_digest(
                cq.data.split(":", 1)[1], data.get("confirm_token", "")
            )
        ):
            await cq.answer(
                "Подтверждение истекло. Выберите страну заново.", show_alert=True
            )
            return
        await cq.answer()
        await operation(
            cq.message,
            state,
            account,
            "region_change",
            country=data["country"],
            address=data["address"],
        )

    @router.callback_query(F.data == "awaddress", StateFilter(AppleAction.confirm))
    async def address_prompt(cq, state):
        await cq.answer()
        await state.set_state(AppleAction.address)
        await edit(
            cq.message,
            "<b>Ваш адрес для App Store</b>\nПришлите одной строкой через точку с запятой:\n<blockquote>Улица; город; индекс; код телефона; номер; штат</blockquote>\n<i>Штат можно не указывать, если он не нужен для выбранной страны.</i>",
            reply_markup=buttons([("Отмена", "awcancel")]),
        )

    @router.message(AppleAction.address, F.text)
    async def save_address(message, state):
        data = await state.get_data()
        account = account_for(message.from_user.id, data.get("account_id"))
        if not account or data.get("expires_at", 0) <= now():
            await state.clear()
            await message.answer("Начните выбор страны заново.")
            return
        parts = [part.strip() for part in message.text.split(";")]
        if (
            len(parts) not in (5, 6)
            or any(not part or len(part) > 120 for part in parts[:5])
            or len(message.text) > 700
            or not re.fullmatch(r"[0-9]{1,8}", parts[3])
            or not re.fullmatch(r"[0-9]{3,15}", parts[4])
        ):
            await message.answer(
                "Проверьте формат: улица; город; индекс; код телефона; номер; штат."
            )
            return
        address = {
            **data["address"],
            **dict(
                zip(
                    ("street", "city", "postalCode", "phoneArea", "phoneNumber"),
                    parts[:5],
                )
            ),
            "state": parts[5] if len(parts) == 6 else None,
        }
        note = await message.answer("<b>Адрес обновлён</b>")
        await confirmation(note, state, account, data["country"], address)

    @router.message(AppleAction.code, F.text)
    async def verify_code(message, state):
        data = await state.get_data()
        account = account_for(message.from_user.id, data.get("account_id"))
        if not account or data.get("expires_at", 0) <= now():
            await state.clear()
            await message.answer("Время подтверждения истекло. Начните запрос заново.")
            return
        code = message.text.strip()
        try:
            await message.delete()
        except Exception:
            pass
        if not re.fullmatch(r"[0-9]{6}", code):
            await message.answer("Пришлите ровно 6 цифр кода Apple.")
            return
        note = await message.answer("<b>Проверяю код Apple…</b>")
        await operation(
            note, state, account, data["action"], **data["fields"], code=code
        )

    @router.callback_query(F.data == "awcancel")
    async def cancel(cq, state):
        data = await state.get_data()
        account = account_for(cq.from_user.id, data.get("account_id"))
        await state.clear()
        if account:
            await apple.stop_worker(account["id"])
        await cq.answer("Отменено")
        account = db.account_of(cq.from_user.id)
        await edit(
            cq.message, ui.my_account(account), reply_markup=ui.my_account_kb(account)
        )

    @router.callback_query(F.data == "awsupport")
    async def support(cq):
        await cq.answer()
        kb = InlineKeyboardBuilder()
        kb.button(text="Управлять Apple ID", url="https://account.apple.com")
        kb.button(text="Правила смены страны", url="https://support.apple.com/118283")
        kb.button(text="Мой Apple ID", callback_data="me")
        kb.adjust(1)
        await edit(
            cq.message,
            "<b>Настройки Apple ID</b>\nСмена страны доступна в разделе «Личная информация → Страна/регион».",
            reply_markup=kb.as_markup(),
        )

    return router
