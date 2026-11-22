from __future__ import annotations

import html
from datetime import datetime, timezone
from typing import Any, Iterable, Sequence

from aiogram.types import InlineKeyboardButton, InlineKeyboardMarkup
from aiogram.utils.keyboard import InlineKeyboardBuilder

from .anim import bar
from .config import Config
from .ota import human_size

PAGE_SIZE = 8
LINE = "━" * 16
THIN = "┈" * 16

PLAN_TITLES = {
    "single": "Разовая установка",
    "week": "Подписка на неделю",
    "month": "Подписка на месяц",
    "halfyear": "Подписка на полгода",
    "year": "Подписка на год",
    "api": "Доступ к Signum API на 30 дней",
    "api100": "100 дополнительных загрузок API",
    "api500": "500 дополнительных загрузок API",
    "api1000": "1000 дополнительных загрузок API",
}

STATUS = {
    "ready": ("✅", "На связи"),
    "need2fa": ("🔐", "Ждёт код из SMS"),
    "error": ("⚠️", "Нужно войти заново"),
    "new": ("⏳", "Не проверен"),
}


def esc(value: Any) -> str:
    return html.escape(str(value or ""))


def when(ts: int, with_time: bool = False) -> str:
    if not ts:
        return "—"
    moment = datetime.fromtimestamp(ts, timezone.utc).astimezone()
    return moment.strftime("%d.%m.%Y, %H:%M" if with_time else "%d.%m.%Y")


def row(icon: str, label: str, value: str) -> str:
    return f"{icon} <b>{label}</b>  ·  {value}"


def back(target: str = "menu", text: str = "◀︎ Назад") -> InlineKeyboardButton:
    return InlineKeyboardButton(text=text, callback_data=target)


def back_kb(target: str = "menu", text: str = "◀︎ Назад") -> InlineKeyboardMarkup:
    return InlineKeyboardMarkup(inline_keyboard=[[back(target, text)]])


# --- главный экран --------------------------------------------------------
def main_menu(is_admin: bool, purchases: bool = True) -> InlineKeyboardMarkup:
    kb = InlineKeyboardBuilder()
    kb.button(text="▦ Каталог Signum", callback_data="cat")
    kb.button(text="⌕ Найти приложение", callback_data="srch")
    if purchases:
        kb.button(text="🧾 Мои покупки", callback_data="pur:1")
    kb.button(text="📥 Мои загрузки", callback_data="hist")
    kb.button(text="🍏 Мой Apple ID", callback_data="me")
    kb.button(text="💎 Подписка", callback_data="buy")
    kb.button(text="ℹ️ Как это работает", callback_data="how")
    kb.button(text="Signum API", callback_data="api")
    if is_admin:
        kb.button(text="🛠 Админ-панель", callback_data="adm")
    kb.adjust(2, 2, 2, 1, 1) if purchases else kb.adjust(2, 2, 1, 1, 1)
    return kb.as_markup()


def greeting(name: str, quota: str) -> str:
    return (
        f"<b>Твоя библиотека iOS — Signum</b>\n\n"
        f"Привет, {esc(name)}. Найди нужное приложение,\n"
        f"выбери сборку и получи <b>IPA</b> или ссылку\n"
        f"на установку для iPhone.\n\n"
        f"{quota}\n\n"
        f"<i>Начни с каталога или поиска ↓</i>"
    )


def quota_line(
    cfg: Config, free_left: int, credits: int, sub_until: int, shared: int = 1
) -> str:
    if sub_until:
        return f"💎 <b>Подписка активна</b>\nДо {when(sub_until)}"
    used = cfg.free_per_week - free_left
    text = (
        f"🎁 <b>Бесплатные установки</b>\n"
        f"{bar(free_left, cfg.free_per_week)}  {free_left} из {cfg.free_per_week}"
    )
    if credits:
        text += f"\n🎟 Купленных скачиваний: <b>{credits}</b>"
    elif used and not free_left:
        text += "\n<i>Бесплатные скачивания на этой неделе закончились.</i>"
    if shared > 1:
        text += "\n🔗 <i>Для одного Apple ID бесплатный лимит общий.</i>"
    return text


def how_it_works(cfg: Config, chat_mb: int) -> str:
    return (
        f"ℹ️ <b>Как это работает</b>\n{LINE}\n"
        f"<b>1.</b> Подключаете свой <b>Apple ID</b> — приложения\n"
        f"      качаются с вашего аккаунта.\n"
        f"<b>2.</b> Находите нужное: каталог, поиск или\n"
        f"      ваши покупки.\n"
        f"<b>3.</b> 📥 <b>Скачать .ipa</b> — пришлю файлом\n"
        f"      (до {chat_mb} МБ).\n"
        f"<b>4.</b> 📲 <b>Установить</b> — открываете ссылку\n"
        f"      в Safari на iPhone и жмёте «Установить».\n"
        f"{THIN}\n"
        f"🎁 Бесплатно — {cfg.free_per_week} приложения в неделю,\n"
        f"      размер до {cfg.max_mb_free // 1024} ГБ.\n"
        f"💎 По подписке — без лимита на количество\n"
        f"      и файлы до {cfg.max_mb_sub // 1024} ГБ."
    )


# --- карточка приложения --------------------------------------------------
def app_card(app: dict[str, Any], *, extra: str = "") -> str:
    rows = [f"📱 <b>{esc(app.get('title'))}</b>"]
    if app.get("artist"):
        rows.append(f"<i>{esc(app['artist'])}</i>")
    rows.append(LINE)
    if app.get("version"):
        rows.append(row("🏷", "Версия", esc(app["version"])))
    if app.get("min_ios"):
        rows.append(row("🍏", "Нужна iOS", f"{esc(app['min_ios'])}+"))
    if app.get("size"):
        rows.append(row("💾", "Размер", human_size(int(app["size"]))))
    rows.append(extra or "\n<i>Выберите, как получить приложение:</i>")
    return "\n".join(rows)


def app_kb(
    adam_id: str, *, back_to: str = "menu", version: str = ""
) -> InlineKeyboardMarkup:
    suffix = f":{version}" if version else ""
    kb = InlineKeyboardBuilder()
    kb.button(text="📥 Скачать .ipa", callback_data=f"dl:{adam_id}{suffix}")
    kb.button(text="📲 Установить на iPhone", callback_data=f"ota:{adam_id}{suffix}")
    if not version:
        kb.button(text="◷ Другие версии", callback_data=f"ver:{adam_id}:0")
    kb.button(text="⌕ Новый поиск", callback_data="srch")
    kb.button(text="◀︎ Назад", callback_data=back_to)
    if back_to != "menu":
        kb.button(text="⌂ Главное меню", callback_data="menu")
    kb.adjust(1, 1, 1, 2, 1) if not version else kb.adjust(1, 1, 2, 1)
    return kb.as_markup()


# --- списки ---------------------------------------------------------------
def groups_kb(groups: Sequence[dict[str, Any]]) -> InlineKeyboardMarkup:
    kb = InlineKeyboardBuilder()
    for group in groups:
        kb.button(
            text=f"📂 {group.get('title') or group.get('id')}",
            callback_data=f"cg:{group.get('id')}:0",
        )
    kb.button(text="◀︎ Назад", callback_data="menu")
    kb.adjust(2)
    return kb.as_markup()


def page_slice(
    items: Sequence[Any], page: int, per_page: int = PAGE_SIZE
) -> tuple[list[Any], int, int]:
    """Кусок списка на страницу: сама страница и сколько их всего."""
    total = max(1, (len(items) + per_page - 1) // per_page)
    page = max(0, min(page, total - 1))
    return list(items[page * per_page : (page + 1) * per_page]), page, total


def nav_kb(
    items: Sequence[tuple[str, str]],
    page: int,
    total: int,
    page_cb: str,
    back_to: str = "menu",
) -> InlineKeyboardMarkup:
    kb = InlineKeyboardBuilder()
    for title, data in items:
        kb.button(text=title, callback_data=data)
    kb.adjust(1)
    nav = InlineKeyboardBuilder()
    if total > 1:
        nav.button(text="◀︎", callback_data=f"{page_cb}:{page - 1}" if page else "noop")
        nav.button(text=f"{page + 1} / {total}", callback_data="noop")
        nav.button(
            text="▶︎",
            callback_data=f"{page_cb}:{page + 1}" if page + 1 < total else "noop",
        )
        nav.adjust(3)
    tail = InlineKeyboardBuilder()
    tail.button(text="◀︎ Назад", callback_data=back_to)
    if back_to != "menu":
        tail.button(text="⌂ Меню", callback_data="menu")
    tail.adjust(2)
    kb.attach(nav)
    kb.attach(tail)
    return kb.as_markup()


def paged_kb(
    items: Sequence[tuple[str, str]],
    page: int,
    page_cb: str,
    *,
    back_to: str = "menu",
    per_page: int = PAGE_SIZE,
) -> tuple[list[tuple[str, str]], InlineKeyboardMarkup]:
    """Страница списка + клавиатура с пролистыванием."""
    chunk, page, total = page_slice(items, page, per_page)
    return chunk, nav_kb(chunk, page, total, page_cb, back_to)


# --- деньги ---------------------------------------------------------------
def paywall(
    cfg: Config, free_left: int, credits: int, sub_until: int, shared: int = 1
) -> tuple[str, InlineKeyboardMarkup]:
    text = (
        f"💎 <b>Подписка</b>\n{LINE}\n"
        f"{quota_line(cfg, free_left, credits, sub_until, shared)}\n{THIN}\n"
        f"<b>По подписке</b>\n"
        f"• Неограниченное количество скачиваний\n"
        f"• Приложения до {cfg.max_mb_sub // 1024} ГБ\n"
        f"• Ссылки действуют {cfg.link_ttl_paid_hours} ч.\n\n"
        f"<b>Одно скачивание</b> — {cfg.price_label('single')}\n"
        f"Без подписки, файл до {cfg.max_mb_sub // 1024} ГБ.\n\n"
        f"Выберите вариант:"
    )
    kb = InlineKeyboardBuilder()
    kb.button(
        text=f"🎟 Одна установка · {cfg.price_label('single')}",
        callback_data="pay:single",
    )
    kb.button(text=f"7 дней · {cfg.price_label('week')}", callback_data="pay:week")
    kb.button(text=f"30 дней · {cfg.price_label('month')}", callback_data="pay:month")
    kb.button(
        text=f"180 дней · {cfg.price_label('halfyear')}", callback_data="pay:halfyear"
    )
    kb.button(text=f"365 дней · {cfg.price_label('year')}", callback_data="pay:year")
    kb.button(
        text=f"Signum API · {cfg.price_label('api')} / 30 дней", callback_data="api"
    )
    kb.button(text="◀︎ Назад", callback_data="menu")
    kb.adjust(1, 2, 2, 1, 1)
    return text, kb.as_markup()


# --- история --------------------------------------------------------------
def history_text(rows: Iterable[Any]) -> str:
    rows = list(rows)
    if not rows:
        return (
            f"📥 <b>Мои загрузки</b>\n{LINE}\n"
            "Пока пусто.\n<i>Загляните в каталог или поиск.</i>"
        )
    icons = {"ipa": "📥", "ota": "📲"}
    lines = [f"📥 <b>Мои загрузки</b>\n{LINE}"]
    for item in rows:
        mark = "" if item["status"] == "ok" else " ⚠️"
        lines.append(
            f"{icons.get(item['kind'], '•')} <b>{esc(item['title'])}</b>{mark}\n"
            f"    <i>{when(item['created_at'], with_time=True)}</i>"
        )
    lines.append(f"{THIN}\n<i>Нажмите на приложение, чтобы получить снова.</i>")
    return "\n".join(lines)


# --- своя страница Apple ID ----------------------------------------------
def my_account(item: Any | None) -> str:
    if item is None:
        return (
            f"🍏 <b>Мой Apple ID</b>\n{LINE}\n"
            "Приложения качаются <b>с вашего аккаунта</b> —\n"
            "как если бы вы нажали «Загрузить» в App Store.\n"
            f"{THIN}\n"
            "🔒 Пароль нужен только для входа в App Store.\n"
            "В аккаунте доступны ваши покупки и страна App Store.\n"
            "Отключить можно в любой момент — одной кнопкой.\n\n"
            "<i>Нажмите «Подключить» — это три шага.</i>"
        )
    icon, label = STATUS.get(item["status"], ("•", item["status"]))
    lines = [
        f"🍏 <b>Мой Apple ID</b>\n{LINE}",
        row("📧", "Почта", f"<code>{esc(item['email'])}</code>"),
        row(icon, "Статус", label),
        row("📥", "Скачано", f"{item['uses']} шт."),
        row("🕘", "Последний раз", when(item["last_used"], with_time=True)),
    ]
    if item["note"] and item["status"] != "ready":
        lines.append(
            f"{THIN}\n⚠️ <i>Проверьте подключение или войдите в Apple ID заново.</i>"
        )
    if "kbsync_warmed_at" in item.keys() and item["kbsync_warmed_at"]:
        lines.append(
            row(
                "⚡", "Готов к загрузке", when(item["kbsync_warmed_at"], with_time=True)
            )
        )
    if "country_name" in item.keys() and item["country_name"]:
        lines.append(row("🌍", "Страна App Store", esc(item["country_name"])))
    return "\n".join(lines)


def my_account_kb(item: Any | None) -> InlineKeyboardMarkup:
    kb = InlineKeyboardBuilder()
    if item is None:
        kb.button(text="➕ Подключить Apple ID", callback_data="acadd")
    else:
        kb.button(text="🧾 История покупок", callback_data="pur:1")
        kb.button(text="🌍 Страна App Store", callback_data="awregion")
        kb.button(text="Сменить страну", callback_data="awcountries:0")
        kb.button(text="🔄 Проверить связь", callback_data=f"act:{item['id']}")
        if item["status"] != "ready":
            kb.button(text="🔐 Войти заново", callback_data="acadd")
        kb.button(text="🚪 Отключить", callback_data=f"acd:{item['id']}")
    kb.button(text="◀︎ Назад", callback_data="menu")
    kb.adjust(1)
    return kb.as_markup()


# --- админка: Apple ID ----------------------------------------------------
def accounts_text(rows: Iterable[Any]) -> str:
    rows = list(rows)
    if not rows:
        return (
            f"🛠 <b>Панель админа</b>\n{LINE}\n"
            "🍏 Ни одного Apple ID.\n\n"
            "<i>Без аккаунта бот не сможет ничего скачать.</i>\n"
            "Нажмите «Добавить Apple ID» — проведу по шагам."
        )
    ready = sum(1 for item in rows if item["status"] == "ready")
    lines = [
        f"🛠 <b>Панель админа</b>\n{LINE}",
        f"🍏 Apple ID: <b>{len(rows)}</b>  ·  на связи: <b>{ready}</b>",
        THIN,
    ]
    for item in rows:
        icon, label = STATUS.get(item["status"], ("•", item["status"]))
        lines.append(
            f"{icon} <code>#{item['id']}</code> {esc(item['email'])} — {label}"
        )
    lines.append(f"{THIN}\n<i>Нажмите на аккаунт, чтобы открыть карточку.</i>")
    return "\n".join(lines)


def accounts_kb(rows: Iterable[Any]) -> InlineKeyboardMarkup:
    kb = InlineKeyboardBuilder()
    for item in rows:
        icon = STATUS.get(item["status"], ("•", ""))[0]
        kb.button(text=f"{icon} {item['email']}", callback_data=f"ac:{item['id']}")
    kb.adjust(1)
    tail = InlineKeyboardBuilder()
    tail.button(text="➕ Добавить Apple ID", callback_data="acadd")
    tail.button(text="📊 Статистика", callback_data="stats")
    tail.button(text="🔄 Обновить", callback_data="acc")
    tail.button(text="◀︎ Назад", callback_data="menu")
    tail.adjust(1, 2, 1)
    kb.attach(tail)
    return kb.as_markup()


def account_card(item: Any) -> str:
    icon, label = STATUS.get(item["status"], ("•", item["status"]))
    lines = [
        f"🍏 <b>Apple ID #{item['id']}</b>\n{LINE}",
        row("📧", "Почта", f"<code>{esc(item['email'])}</code>"),
        row(icon, "Статус", label),
        row("📥", "Выдал", f"{item['uses']} шт."),
        row("🕘", "Работал", when(item["last_used"], with_time=True)),
    ]
    if item["note"]:
        lines.append(f"{THIN}\n⚠️ <i>{esc(item['note'])}</i>")
    return "\n".join(lines)


def account_kb(item: Any) -> InlineKeyboardMarkup:
    kb = InlineKeyboardBuilder()
    kb.button(text="🔄 Проверить связь", callback_data=f"act:{item['id']}")
    if item["status"] != "ready":
        kb.button(text="🔐 Войти заново", callback_data=f"acl:{item['id']}")
    kb.button(text="🗑 Удалить", callback_data=f"acd:{item['id']}")
    kb.button(text="◀︎ К списку", callback_data="acc")
    kb.adjust(1)
    return kb.as_markup()


# --- мастер входа ---------------------------------------------------------
def wizard(step: int, total: int, title: str, body: str) -> str:
    return (
        f"🔐 <b>{title}</b>\n{bar(step, total)}  шаг {step} из {total}\n{LINE}\n{body}"
    )


WIZARD_EMAIL = "Введите <b>почту Apple ID</b>."
WIZARD_PASSWORD = (
    "Введите <b>пароль Apple ID</b>.\n\n<i>Сообщение с паролем будет удалено.</i>"
)
WIZARD_CODE = "Введите <b>6 цифр</b> с устройства Apple или из SMS."


def cancel_kb(target: str = "acc") -> InlineKeyboardMarkup:
    return back_kb(target, "✖️ Отмена")


# --- результаты выдачи ----------------------------------------------------
def ready_ota(title: str, size: int, ttl: int, email: str, encrypted: bool) -> str:
    """Инструкция к установке по воздуху.

    Сборка из App Store расшифровывается лицензией того Apple ID, который её
    купил. Поэтому на телефоне должна быть включена та же учётка — без этого
    iOS ответит «не удаётся установить», сколько ни повторяй.
    """
    head = (
        f"✅ <b>Готово к установке</b>\n{LINE}\n"
        f"📱 <b>{esc(title)}</b>\n💾 {human_size(size)}\n{THIN}\n"
    )
    if not encrypted:
        return head + (
            "<b>1.</b> Откройте ссылку <b>в Safari на iPhone</b>.\n"
            "<b>2.</b> Нажмите «Установить» и вернитесь на экран «Домой».\n\n"
            f"<i>Ссылка живёт {ttl} ч.</i>"
        )
    return head + (
        "<b>1.</b> На iPhone: Настройки → <b>App Store</b> →\n"
        "     нажмите на свой Apple ID → <b>Выйти</b>.\n"
        "     <i>iCloud не трогайте, выходим только из App Store.</i>\n"
        f"<b>2.</b> Войдите под <code>{esc(email)}</code>\n"
        "<b>3.</b> Откройте ссылку ниже <b>в Safari</b> → «Установить».\n"
        "<b>4.</b> После установки можно вернуть свою учётку —\n"
        "     приложение останется на месте.\n"
        f"{THIN}\n"
        "<blockquote>Приложение устанавливается под этим Apple ID.\n"
        f"Ссылка живёт {ttl} ч.</blockquote>"
    )


def send_failed(title: str, reason: str) -> str:
    return (
        f"⚠️ <b>Файл не ушёл</b>\n{LINE}\n"
        f"📱 <b>{esc(title)}</b>\n{THIN}\n"
        f"Telegram временно не принял файл.\n\n<i>Приложение сохранено — попробуйте ещё раз.</i>"
    )


def cancel_kb_job(job_id: str) -> InlineKeyboardMarkup:
    return InlineKeyboardMarkup(
        inline_keyboard=[
            [InlineKeyboardButton(text="✖️ Отменить", callback_data=f"stop:{job_id}")]
        ]
    )


def cancelled(title: str) -> str:
    return (
        f"✖️ <b>Загрузка отменена</b>\n{LINE}\n"
        f"📱 <b>{esc(title)}</b>\n{THIN}\n"
        f"<i>Бесплатная установка не потрачена.</i>"
    )


def failed(reason: str, hint: str = "") -> str:
    messages = (
        ("поиск временно", "Поиск сейчас недоступен. Попробуйте ещё раз."),
        ("сначала подключите", "Сначала подключите свой Apple ID."),
        ("apple id не на связи", "Откройте «Мой Apple ID» и войдите заново."),
        ("войдите", "Войдите в Apple ID заново."),
        (
            "бесплатные скачивания закончились",
            "Бесплатные скачивания на этой неделе закончились.",
        ),
        (
            "оплаченные скачивания уже используются",
            "Дождитесь завершения текущих скачиваний.",
        ),
        (
            "подписка закончилась",
            "Подписка закончилась. Продлите её и повторите скачивание.",
        ),
        ("загрузки сейчас недоступны", "Загрузки сейчас недоступны. Попробуйте позже."),
        (
            "недостаточно свободного места",
            "Загрузка сейчас недоступна. Попробуйте позже.",
        ),
        ("слишком большой", "Этот файл слишком большой для отправки в чат."),
        ("установка сейчас недоступна", "Установка сейчас недоступна."),
        ("not purchased", "Приложения нет в покупках этого Apple ID."),
        ("not available", "Приложение недоступно для этого Apple ID."),
    )
    reason = next(
        (message for marker, message in messages if marker in reason.lower()),
        "Не удалось выполнить запрос. Попробуйте ещё раз.",
    )
    return f"⚠️ <b>Не получилось</b>\n{LINE}\n{esc(reason)}" + (
        f"\n{THIN}\n<i>{esc(hint)}</i>" if hint else ""
    )


def admin_panel(
    data: dict[str, int], files: tuple[int, int], money: float, unit: str
) -> str:
    return (
        f"🛠 <b>Админ-панель</b>\n{LINE}\n"
        + row(
            "👥", "Пользователей", f"{data['users']}  (+{data['users_week']} за неделю)"
        )
        + "\n"
        + row(
            "🍏",
            "Apple ID",
            f"{data['accounts']}  ·  на связи {data['accounts_ready']}",
        )
        + "\n"
        + row("📥", "Выдач", f"{data['downloads']}  (за неделю {data['week']})")
        + "\n"
        + row("💾", "Файлов", f"{files[0]}  ·  {human_size(files[1])}")
        + "\n"
        + row("💰", "Оплат", f"{money:g} {unit}")
        + f"\n{THIN}\n<b>API</b>\nКлиентов: <b>{data.get('api_clients', 0)}</b> · ключей: <b>{data.get('api_keys', 0)}</b>\nВ очереди: <b>{data.get('api_jobs', 0)}</b>"
        + f"\n{THIN}\n<i>Выберите раздел:</i>"
    )


def admin_kb(bot_maintenance=False, api_maintenance=False) -> InlineKeyboardMarkup:
    kb = InlineKeyboardBuilder()
    kb.button(text="👥 Пользователи", callback_data="admu:0")
    kb.button(text="🍏 Apple ID", callback_data="acc")
    kb.button(text="💾 Файлы", callback_data="admf")
    kb.button(text="📣 Рассылка", callback_data="admb")
    kb.button(text="🔐 API и ключи", callback_data="am:0")
    kb.button(text="Состояние сервисов", callback_data="admin:health")
    kb.button(text="Оплаты", callback_data="admin:payments")
    kb.button(text="Общие Apple ID", callback_data="admin:fraud")
    kb.button(text="Резервная копия", callback_data="admin:backup")
    kb.button(
        text="Возобновить бота" if bot_maintenance else "Техработы бота",
        callback_data="maintenance:bot",
    )
    kb.button(
        text="Возобновить API" if api_maintenance else "Техработы API",
        callback_data="maintenance:api",
    )
    kb.button(text="◀︎ Назад", callback_data="menu")
    kb.adjust(2, 2, 2)
    return kb.as_markup()


def users_text(rows: Sequence[Any], total: int, page: int, per_page: int) -> str:
    lines = [f"👥 <b>Пользователи</b>\n{LINE}", f"Всего: <b>{total}</b>", THIN]
    for item in rows:
        who = (
            f"@{item['username']}"
            if item["username"]
            else (item["name"] or item["tg_id"])
        )
        flags = []
        if item["sub_until"] > 0:
            flags.append("💎")
        if item["credits"]:
            flags.append(f"🎟{item['credits']}")
        lines.append(
            ("🚫 " if item["blocked"] else "")
            + f"<b>{esc(who)}</b>  ·  📥 {item['uses']}  {' '.join(flags)}"
        )
    if not rows:
        lines.append("<i>Пусто.</i>")
    lines.append(f"{THIN}\nСтраница {page + 1} из {max(1, -(-total // per_page))}")
    return "\n".join(lines)


def user_card(item: Any, uses: int, sub_left: int) -> str:
    who = f"@{item['username']}" if item["username"] else (item["name"] or "—")
    return (
        f"👤 <b>{esc(who)}</b>\n{LINE}\n"
        + row("🪪", "ID", f"<code>{item['tg_id']}</code>")
        + "\n"
        + row("📥", "Выдач", str(uses))
        + "\n"
        + row("💎", "Подписка", when(sub_left) if sub_left else "нет")
        + "\n"
        + row("🎟", "Установок в запасе", str(item["credits"]))
        + "\n"
        + row("🚦", "Доступ", "🚫 закрыт" if item["blocked"] else "✅ открыт")
    )


def user_kb(item: Any) -> InlineKeyboardMarkup:
    kb = InlineKeyboardBuilder()
    kb.button(text="API и ключи", callback_data=f"au:{item['tg_id']}")
    kb.button(text="💎 +30 дней", callback_data=f"admus:{item['tg_id']}")
    kb.button(text="🎟 +1 установка", callback_data=f"admuc:{item['tg_id']}")
    kb.button(
        text="✅ Разблокировать" if item["blocked"] else "🚫 Заблокировать",
        callback_data=f"admub:{item['tg_id']}",
    )
    kb.button(text="◀︎ К списку", callback_data="admu:0")
    kb.adjust(1, 2, 1, 1)
    return kb.as_markup()


def files_text(count: int, size: int, free_ttl: int, paid_ttl: int) -> str:
    return (
        f"💾 <b>Файлы на сервере</b>\n{LINE}\n"
        + row("📦", "Штук", str(count))
        + "\n"
        + row("🗄", "Занято", human_size(size))
        + f"\n{THIN}\n<i>Бесплатные ссылки — {free_ttl} ч. Подписка и разовая оплата — {paid_ttl} ч.</i>"
    )


def files_kb() -> InlineKeyboardMarkup:
    kb = InlineKeyboardBuilder()
    kb.button(text="🧹 Убрать просроченные", callback_data="admfc")
    kb.button(text="🔥 Стереть всё", callback_data="admfa")
    kb.button(text="◀︎ Назад", callback_data="adm")
    kb.adjust(1)
    return kb.as_markup()


BROADCAST_ASK = (
    f"📣 <b>Рассылка</b>\n{LINE}\n"
    "Пришлите текст — покажу, как он будет выглядеть,\n"
    "и только потом спрошу подтверждение.\n\n"
    "<i>Разметка Telegram работает: жирный, курсив, ссылки.</i>"
)


def too_big(size: int, limit: int, subscriber: bool) -> str:
    tail = (
        "Выберите приложение меньшего размера."
        if subscriber
        else "По подписке можно скачивать файлы большего размера."
    )
    return (
        f"🚫 <b>Слишком большое приложение</b>\n{LINE}\n"
        + row("💾", "Размер", human_size(size))
        + "\n"
        + row("📏", "Ваш предел", human_size(limit))
        + f"\n{THIN}\n<i>{tail}</i>"
    )


def stats_text(data: dict[str, int], money: float, unit: str) -> str:
    return (
        f"📊 <b>Статистика</b>\n{LINE}\n"
        + row("👥", "Пользователей", str(data["users"]))
        + "\n"
        + row("🍏", "Живых Apple ID", str(data["accounts"]))
        + "\n"
        + row("📥", "Выдач всего", f"{data['downloads']}  (за неделю {data['week']})")
        + "\n"
        + row("💰", "Оплат", f"{money:g} {unit}")
    )
