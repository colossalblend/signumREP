from __future__ import annotations

from aiogram import F, Router
from aiogram.fsm.state import State, StatesGroup
from aiogram.utils.keyboard import InlineKeyboardBuilder

from . import ui
from .api_keys import FULL_SCOPES, audit, create_key, keys_for, update_key
from .db import now

PAGE = 8
STATES = {"active": "Работает", "paused": "На паузе", "revoked": "Отозван"}


class EditApi(StatesGroup):
    name = State()
    key_quota = State()
    key_days = State()
    quota = State()
    days = State()
    search = State()


def build_management(deps, edit):
    router = Router(name="api-management")
    db, cfg = deps.db, deps.cfg
    router.callback_query.filter(F.message.chat.type == "private")
    router.message.filter(F.chat.type == "private")

    def permitted(actor, owner):
        return actor == owner or actor in cfg.admins

    async def show_keys(message, owner, page=0, admin=False):
        rows = keys_for(db, owner)
        rows = [row for row in rows if row["state"] != "revoked"]
        kb = InlineKeyboardBuilder()
        for row in rows[page * PAGE : (page + 1) * PAGE]:
            state = "Истёк" if row["expires_at"] <= now() else STATES[row["state"]]
            kb.button(text=f"{row['label']} · {state}", callback_data=f"ki:{row['id']}")
        kb.adjust(1)
        nav = InlineKeyboardBuilder()
        if page:
            nav.button(text="◀︎", callback_data=f"kp:{owner}:{page - 1}")
        if (page + 1) * PAGE < len(rows):
            nav.button(text="▶︎", callback_data=f"kp:{owner}:{page + 1}")
        nav.adjust(2)
        kb.attach(nav)
        tail = InlineKeyboardBuilder()
        tail.button(text="➕ Создать ключ", callback_data=f"kn:{owner}:full")
        tail.button(text="Ключ для просмотра", callback_data=f"kn:{owner}:read")
        tail.button(text="◀︎ Назад", callback_data=f"au:{owner}" if admin else "api")
        tail.adjust(1)
        kb.attach(tail)
        text = (
            f"🔐 <b>API-ключи</b>\n{ui.LINE}\nАктивных и приостановленных: <b>{len(rows)}</b>\n\n<blockquote>Выберите ключ, чтобы изменить его лимит, срок или приостановить доступ.</blockquote>"
            if rows
            else f"🔐 <b>API-ключи</b>\n{ui.LINE}\nКлючей пока нет. Создайте первый."
        )
        await edit(message, text, reply_markup=kb.as_markup())

    @router.callback_query(F.data.startswith("kp:"))
    async def key_page(cq, state):
        _, owner, page = cq.data.split(":")
        owner = int(owner)
        if not permitted(cq.from_user.id, owner):
            await cq.answer("Нет доступа", show_alert=True)
            return
        await cq.answer()
        await state.clear()
        await show_keys(cq.message, owner, max(0, int(page)), cq.from_user.id != owner)

    def owned_key(cq, identifier):
        key = db.one("select * from api_keys where id=?", (identifier,))
        return key if key and permitted(cq.from_user.id, key["tg_id"]) else None

    async def show_key(message, key):
        owner = key["tg_id"]
        client = db.one(
            "select api_remaining,api_extra_remaining from users where tg_id=?",
            (owner,),
        )
        state = (
            "Истёк"
            if key["expires_at"] <= now() and key["state"] != "revoked"
            else STATES[key["state"]]
        )
        quota = (
            "По балансу клиента"
            if key["quota_remaining"] is None
            else str(key["quota_remaining"])
        )
        text = f"🔐 <b>{ui.esc(key['label'])}</b>\n{ui.LINE}\nСтатус: <b>{state}</b>\nЛимит ключа: <b>{quota}</b>\nБаланс клиента: <b>{client['api_remaining'] + client['api_extra_remaining']}</b>\nДействует до: <b>{ui.when(key['expires_at'])}</b>\nПоследнее использование: {ui.when(key['last_used'], with_time=True)}\n\nID ключа: <code>{key['id']}</code>\n<i>Секрет ключа после создания не показывается.</i>"
        kb = InlineKeyboardBuilder()
        if key["state"] != "revoked":
            kb.button(
                text="Возобновить" if key["state"] == "paused" else "Пауза",
                callback_data=f"km:{'active' if key['state'] == 'paused' else 'paused'}:{key['id']}",
            )
            kb.button(text="Лимит загрузок", callback_data=f"kq:{key['id']}")
            kb.button(text="Изменить срок", callback_data=f"kd:{key['id']}")
            kb.button(text="Отозвать", callback_data=f"kr:{key['id']}")
        kb.button(text="◀︎ К ключам", callback_data=f"kp:{owner}:0")
        kb.adjust(2, 2, 1)
        await edit(message, text, reply_markup=kb.as_markup())

    @router.callback_query(F.data.startswith("ki:"))
    async def key_card(cq, state):
        key = owned_key(cq, cq.data.split(":")[1])
        if not key:
            await cq.answer("Ключ не найден", show_alert=True)
            return
        await cq.answer()
        await state.clear()
        await show_key(cq.message, key)

    @router.callback_query(F.data.startswith("kn:"))
    async def new_key(cq, state):
        _, owner, profile = cq.data.split(":")
        owner = int(owner)
        user = db.one("select api_until from users where tg_id=?", (owner,))
        if (
            not permitted(cq.from_user.id, owner)
            or not user
            or (user["api_until"] <= now() and cq.from_user.id not in cfg.admins)
        ):
            await cq.answer("Сначала продлите API", show_alert=True)
            return
        await cq.answer()
        await state.set_state(EditApi.name)
        await state.update_data(owner=owner, profile=profile)
        await edit(
            cq.message,
            "<b>Название ключа</b>\nНапример: <code>Мой сайт</code>. До 40 символов.",
            reply_markup=ui.cancel_kb(f"kp:{owner}:0"),
        )

    @router.message(EditApi.name)
    async def save_name(message, state):
        data = await state.get_data()
        if not permitted(message.from_user.id, data["owner"]):
            await state.clear()
            return
        user = db.one("select api_until from users where tg_id=?", (data["owner"],))
        expiry = now() + cfg.api_key_days * 86400
        if message.from_user.id not in cfg.admins:
            expiry = min(expiry, user["api_until"])
        try:
            _, secret = create_key(
                db,
                data["owner"],
                (message.text or "").strip(),
                scopes="read" if data["profile"] == "read" else FULL_SCOPES,
                expires_at=expiry,
                actor=message.from_user.id,
            )
        except ValueError:
            await message.answer(
                "Введите название до 40 символов. Всего можно создать до 20 ключей."
            )
            return
        await state.clear()
        await message.answer(
            f"✅ <b>Ключ создан</b>\n<pre>{secret}</pre>\n<blockquote>Нажмите на ключ, чтобы скопировать. Сохраните его: повторно посмотреть секрет нельзя.</blockquote>",
            reply_markup=ui.back_kb(f"kp:{data['owner']}:0", "К моим ключам"),
        )

    @router.callback_query(F.data.startswith("km:"))
    async def key_state(cq):
        _, status, identifier = cq.data.split(":")
        key = owned_key(cq, identifier)
        if not key or status not in {"active", "paused"}:
            await cq.answer("Нет доступа", show_alert=True)
            return
        try:
            key = update_key(
                db, key["tg_id"], identifier, {"state": status}, actor=cq.from_user.id
            )
        except ValueError:
            await cq.answer("Ключ отозван", show_alert=True)
            return
        await cq.answer("Доступ возобновлён" if status == "active" else "Ключ на паузе")
        await show_key(cq.message, key)

    @router.callback_query(F.data.startswith("kr:"))
    async def confirm_revoke(cq):
        key = owned_key(cq, cq.data.split(":")[1])
        if not key:
            return
        await cq.answer()
        kb = InlineKeyboardBuilder()
        kb.button(text="Отозвать ключ", callback_data=f"kx:{key['id']}")
        kb.button(text="Отмена", callback_data=f"ki:{key['id']}")
        kb.adjust(1)
        await edit(
            cq.message,
            f"<b>Отозвать «{ui.esc(key['label'])}»?</b>\nКлюч перестанет работать. Восстановить его нельзя.",
            reply_markup=kb.as_markup(),
        )

    @router.callback_query(F.data.startswith("kx:"))
    async def revoke(cq):
        key = owned_key(cq, cq.data.split(":")[1])
        if not key:
            return
        try:
            update_key(
                db, key["tg_id"], key["id"], {"state": "revoked"}, actor=cq.from_user.id
            )
        except ValueError:
            pass
        await cq.answer("Ключ отозван")
        await show_keys(cq.message, key["tg_id"], admin=cq.from_user.id != key["tg_id"])

    @router.callback_query(F.data.startswith("kq:"))
    @router.callback_query(F.data.startswith("kd:"))
    async def key_number(cq, state):
        key = owned_key(cq, cq.data.split(":")[1])
        if not key or key["state"] == "revoked":
            return
        await cq.answer()
        quota = cq.data.startswith("kq:")
        await state.set_state(EditApi.key_quota if quota else EditApi.key_days)
        await state.update_data(key=key["id"], owner=key["tg_id"])
        text = (
            "<b>Лимит ключа</b>\nВведите остаток загрузок. <code>0</code> запретит новые загрузки; <code>без лимита</code> оставит только общий баланс клиента."
            if quota
            else "<b>Срок ключа</b>\nВведите количество дней от сегодняшней даты: от 1 до 365."
        )
        await edit(cq.message, text, reply_markup=ui.cancel_kb(f"ki:{key['id']}"))

    @router.message(EditApi.key_quota)
    @router.message(EditApi.key_days)
    async def save_key_number(message, state):
        data = await state.get_data()
        if not permitted(message.from_user.id, data["owner"]):
            await state.clear()
            return
        current = await state.get_state()
        try:
            if current == EditApi.key_quota.state:
                raw = (message.text or "").strip().casefold()
                changes = {"quota_remaining": None if raw == "без лимита" else int(raw)}
            else:
                days = int(message.text or "")
                if not 1 <= days <= 365:
                    raise ValueError
                expiry = now() + days * 86400
                if message.from_user.id not in cfg.admins:
                    expiry = min(
                        expiry,
                        db.one(
                            "select api_until from users where tg_id=?",
                            (data["owner"],),
                        )[0],
                    )
                changes = {"expires_at": expiry}
            update_key(
                db, data["owner"], data["key"], changes, actor=message.from_user.id
            )
        except ValueError:
            await message.answer("Проверьте значение и отправьте ещё раз.")
            return
        await state.clear()
        await message.answer(
            "✅ <b>Изменения сохранены</b>",
            reply_markup=ui.back_kb(f"ki:{data['key']}", "Открыть ключ"),
        )

    @router.callback_query(F.data.startswith("am:"))
    async def admin_clients(cq, state):
        if cq.from_user.id not in cfg.admins:
            return
        await cq.answer()
        await state.clear()
        page = max(0, int(cq.data.split(":")[1]))
        rows = db.all(
            "select u.*,(select count(*) from api_keys k where k.tg_id=u.tg_id and k.state!='revoked') keys_count from users u where api_until>0 or exists(select 1 from api_keys k where k.tg_id=u.tg_id) order by u.created_at desc limit ? offset ?",
            (PAGE + 1, page * PAGE),
        )
        kb = InlineKeyboardBuilder()
        for user in rows[:PAGE]:
            who = (
                "@" + user["username"]
                if user["username"]
                else user["name"] or str(user["tg_id"])
            )
            kb.button(
                text=f"{who} · {user['api_remaining'] + user['api_extra_remaining']} загрузок",
                callback_data=f"au:{user['tg_id']}",
            )
        kb.adjust(1)
        nav = InlineKeyboardBuilder()
        if page:
            nav.button(text="◀︎", callback_data=f"am:{page - 1}")
        if len(rows) > PAGE:
            nav.button(text="▶︎", callback_data=f"am:{page + 1}")
        nav.adjust(2)
        kb.attach(nav)
        tail = InlineKeyboardBuilder()
        tail.button(text="Найти клиента", callback_data="asearch")
        tail.button(text="История изменений", callback_data="alog")
        tail.button(text="◀︎ В панель", callback_data="adm")
        tail.adjust(1)
        kb.attach(tail)
        total = db.one(
            "select count(*) from users where api_until>? and api_enabled=1 and blocked=0",
            (now(),),
        )[0]
        text = f"<b>Управление API</b>\n{ui.LINE}\nДействующих клиентов: <b>{total}</b>\n\nВыберите клиента или найдите его по ID."
        await edit(cq.message, text, reply_markup=kb.as_markup())

    async def show_client(message, owner):
        user = db.one("select * from users where tg_id=?", (owner,))
        if not user:
            await edit(message, "Клиент не найден.", reply_markup=ui.back_kb("am:0"))
            return
        status = (
            "Заблокирован"
            if user["blocked"]
            else "Пауза"
            if not user["api_enabled"]
            else "Истёк"
            if user["api_until"] <= now()
            else "Работает"
        )
        text = f"<b>API клиента</b>\n{ui.LINE}\nID: <code>{owner}</code>\nСтатус: <b>{status}</b>\nДоступ до: <b>{ui.when(user['api_until'])}</b>\nОсновных загрузок: <b>{user['api_remaining']}</b>\nДокупленных: <b>{user['api_extra_remaining']}</b>\nКлючей: <b>{len(keys_for(db, owner))}</b>"
        kb = InlineKeyboardBuilder()
        kb.button(text="Ключи", callback_data=f"kp:{owner}:0")
        kb.button(text="Баланс загрузок", callback_data=f"aq:{owner}")
        kb.button(text="Изменить срок API", callback_data=f"at:{owner}")
        kb.button(
            text="Возобновить API" if not user["api_enabled"] else "Пауза API",
            callback_data=f"ap:{owner}",
        )
        kb.button(text="◀︎ К клиентам", callback_data="am:0")
        kb.adjust(2, 2, 1)
        await edit(message, text, reply_markup=kb.as_markup())

    @router.callback_query(F.data.startswith("au:"))
    async def client_card(cq, state):
        if cq.from_user.id not in cfg.admins:
            return
        await cq.answer()
        await state.clear()
        await show_client(cq.message, int(cq.data.split(":")[1]))

    @router.callback_query(F.data.startswith("ap:"))
    async def pause_client(cq):
        if cq.from_user.id not in cfg.admins:
            return
        owner = int(cq.data.split(":")[1])
        with db.conn:
            db.conn.execute(
                "update users set api_enabled=1-api_enabled where tg_id=?", (owner,)
            )
            audit(db, cq.from_user.id, owner, "api_pause_changed")
        await cq.answer("Сохранено")
        await show_client(cq.message, owner)

    @router.callback_query(F.data.startswith("aq:"))
    @router.callback_query(F.data.startswith("at:"))
    async def client_number(cq, state):
        if cq.from_user.id not in cfg.admins:
            return
        owner = int(cq.data.split(":")[1])
        await cq.answer()
        quota = cq.data.startswith("aq:")
        await state.set_state(EditApi.quota if quota else EditApi.days)
        await state.update_data(owner=owner)
        text = (
            "<b>Баланс загрузок</b>\nВведите два числа: основной пакет и докупленные загрузки.\nНапример: <code>1000 250</code>. Это новый остаток, а не прибавка."
            if quota
            else "<b>Срок API</b>\nВведите количество дней от сегодня.\n<code>0</code> завершит доступ; <code>30</code> откроет API на месяц."
        )
        await edit(cq.message, text, reply_markup=ui.cancel_kb(f"au:{owner}"))

    @router.message(EditApi.quota)
    @router.message(EditApi.days)
    async def save_client_number(message, state):
        if message.from_user.id not in cfg.admins:
            await state.clear()
            return
        data = await state.get_data()
        try:
            if await state.get_state() == EditApi.quota.state:
                base, extra = map(int, (message.text or "").split())
                if not 0 <= base <= 1_000_000 or not 0 <= extra <= 1_000_000:
                    raise ValueError
                query = "update users set api_remaining=?,api_extra_remaining=? where tg_id=?"
                values = (base, extra, data["owner"])
                action = "api_quota_changed"
            else:
                days = int(message.text or "")
                if not 0 <= days <= 365:
                    raise ValueError
                query = "update users set api_until=? where tg_id=?"
                values = (now() + days * 86400 if days else 0, data["owner"])
                action = "api_expiry_changed"
            with db.conn:
                db.conn.execute(query, values)
                audit(db, message.from_user.id, data["owner"], action)
        except ValueError:
            await message.answer("Проверьте числа и отправьте ещё раз.")
            return
        await state.clear()
        await message.answer(
            "✅ <b>Баланс сохранён</b>"
            if action == "api_quota_changed"
            else "✅ <b>Срок сохранён</b>",
            reply_markup=ui.back_kb(f"au:{data['owner']}", "Открыть клиента"),
        )

    @router.callback_query(F.data == "asearch")
    async def find_client(cq, state):
        if cq.from_user.id not in cfg.admins:
            return
        await cq.answer()
        await state.set_state(EditApi.search)
        await edit(
            cq.message,
            "<b>Найти клиента</b>\nПришлите Telegram ID или @имя.",
            reply_markup=ui.cancel_kb("am:0"),
        )

    @router.message(EditApi.search)
    async def search_client(message, state):
        if message.from_user.id not in cfg.admins:
            return
        term = (message.text or "").strip().lstrip("@")[:64]
        identifier = int(term) if term.isdigit() and len(term) <= 19 else -1
        if identifier > 2**63 - 1:
            identifier = -1
        user = db.one(
            "select tg_id from users where tg_id=? or lower(username)=lower(?)",
            (identifier, term),
        )
        if not user:
            await message.answer("Клиент не найден. Проверьте ID или имя.")
            return
        await state.clear()
        await message.answer(
            f"Клиент: <code>{user['tg_id']}</code>",
            reply_markup=ui.back_kb(f"au:{user['tg_id']}", "Открыть клиента"),
        )

    @router.callback_query(F.data == "alog")
    async def audit_log(cq):
        if cq.from_user.id not in cfg.admins:
            return
        await cq.answer()
        rows = db.all("select * from api_audit order by id desc limit 8")
        names = {
            "key_created": "Создан ключ",
            "api_pause_changed": "Пауза API",
            "api_quota_changed": "Изменён баланс",
            "api_expiry_changed": "Изменён срок",
        }
        lines = [f"<b>История изменений</b>\n{ui.LINE}"]
        for row in rows:
            action = names.get(row["action"], "Изменён ключ")
            lines.append(
                f"<b>{action}</b> · <code>{row['tg_id']}</code>\n<i>{ui.when(row['created_at'], with_time=True)}</i>"
            )
        if not rows:
            lines.append("Изменений пока нет.")
        await edit(cq.message, "\n\n".join(lines), reply_markup=ui.back_kb("am:0"))

    return router
