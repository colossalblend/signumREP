from __future__ import annotations

import asyncio
import hashlib
import logging
import re
import secrets
import shutil
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from aiogram import Bot, F, Router
from aiogram.filters import Command, CommandStart, StateFilter
from aiogram.exceptions import TelegramBadRequest
from cachetools import TTLCache
from aiogram.fsm.context import FSMContext
from aiogram.fsm.state import State, StatesGroup
from aiogram.types import (
    BufferedInputFile,
    CallbackQuery,
    FSInputFile,
    InputMediaPhoto,
    Message,
)
from aiogram.utils.chat_action import ChatActionSender
from aiogram.utils.keyboard import InlineKeyboardBuilder

from . import ota, ui
from .anim import Progress, waiting
from .cards import Cards
from .config import Config
from .db import Db, now
from .commerce import API_TOPUPS, activate_demo, create_order, issue_api_key
from .production import reserve_storage, release_storage
from .payments import Payments, PaymentError
from .ipatool import Ipatool, IpatoolError, NoAccount
from .sender import FileSender
from .store import Store
from .ownership import OwnedCatalog
from .emoji_ids import has_custom_emoji, replace_custom_emojis
from .management import build_management

USERS_PAGE = 8
# После половины файла отмена уже бессмысленна: трафик потрачен, а докачать
# остаток дешевле, чем начинать сначала.
CANCEL_LIMIT = 50.0


@dataclass
class Job:
    """Одна идущая загрузка: сколько прошло и чем её остановить."""

    tg_id: int
    title: str
    percent: float = 0.0
    proc: Any = None
    cancelled: bool = False
    from_tool: bool = False

    def stop(self) -> bool:
        if self.percent >= CANCEL_LIMIT:
            return False
        self.cancelled = True
        if self.proc is not None and self.proc.returncode is None:
            self.proc.kill()
        return True


class Ask(StatesGroup):
    search = State()


class Auth(StatesGroup):
    email = State()
    password = State()
    code = State()


class Adm(StatesGroup):
    broadcast = State()


@dataclass
class Deps:
    cfg: Config
    db: Db
    store: Store
    tool: Ipatool
    sender: FileSender
    cards: Cards
    payments: Payments | None = None
    apple: Any = None
    search_cache: dict[int, list[dict[str, Any]]] = field(
        default_factory=lambda: TTLCache(maxsize=512, ttl=3600)
    )
    search_terms: dict[int, str] = field(
        default_factory=lambda: TTLCache(maxsize=512, ttl=3600)
    )
    last_list: dict[int, str] = field(
        default_factory=lambda: TTLCache(maxsize=512, ttl=3600)
    )
    running: dict[str, Job] = field(default_factory=dict)
    job_tasks: set[asyncio.Task] = field(default_factory=set)
    developer_cache: dict = field(
        default_factory=lambda: TTLCache(maxsize=512, ttl=1800)
    )
    catalog_cache: dict[str, list[dict[str, Any]]] = field(
        default_factory=lambda: TTLCache(maxsize=512, ttl=3600)
    )
    version_cache: dict[str, list[str]] = field(
        default_factory=lambda: TTLCache(maxsize=128, ttl=600)
    )
    version_labels: dict = field(
        default_factory=lambda: TTLCache(maxsize=4096, ttl=86400)
    )
    broadcast: dict[int, str] = field(
        default_factory=lambda: TTLCache(maxsize=512, ttl=3600)
    )

    def is_admin(self, tg_id: int) -> bool:
        return tg_id in self.cfg.admins

    def quota(self, tg_id: int) -> tuple[int, int, int]:
        """Осталось бесплатных, оплаченных установок и до какого числа подписка."""
        sub = self.db.sub_active(tg_id)
        left = max(0, self.cfg.free_per_week - self.db.week_uses(tg_id))
        return left, self.db.credits(tg_id), sub

    def shared(self, tg_id: int) -> int:
        """Сколько телеграм-аккаунтов делят один Apple ID, включая этот."""
        return len(self.db.family(tg_id))

    def may_install(self, tg_id: int) -> str | None:
        """Чем платим за эту установку: sub / free / credit. None — платить нечем."""
        left, credits, sub = self.quota(tg_id)
        if sub:
            return "sub"
        if left > 0:
            return "free"
        if credits > 0:
            return "credit"
        return None

    def size_limit(self, tg_id: int) -> int:
        """Потолок размера приложения по тарифу."""
        return self.cfg.max_bytes(
            bool(self.db.sub_active(tg_id)) or self.db.credits(tg_id) > 0
        )

    def chat_limit(self, tg_id: int) -> int:
        """Сколько из этого реально влезет файлом в чат."""
        return self.sender.limit(self.size_limit(tg_id))

    def limit_for(self, tg_id: int, kind: str) -> int:
        """Файл едет через Telegram, установка — по своей ссылке: потолки разные."""
        return (
            self.chat_limit(tg_id)
            if kind == "ipa" and not self.cfg.public_base_url
            else self.size_limit(tg_id)
        )


def build_router(deps: Deps) -> Router:  # noqa: C901 - один экран бота, дробить нечего
    router = Router()
    cfg, db, store, tool = deps.cfg, deps.db, deps.store, deps.tool
    from .apple_web import AppleWeb

    if deps.apple is None:
        deps.apple = AppleWeb(cfg, db)
    ownership = OwnedCatalog(tool)

    def admin_only(tg_id: int) -> bool:
        return tg_id in cfg.admins

    def owns(tg_id: int, account: Any) -> bool:
        return account is not None and (
            int(account["tg_id"]) == tg_id or admin_only(tg_id)
        )

    async def swap(
        message: Message,
        text: str,
        kb: Any,
        photo: bytes | None = None,
        key: str = "",
    ) -> None:
        """Меняет экран на месте.

        Telegram не умеет превращать текстовое сообщение в картинку и обратно
        одной правкой, поэтому на стыке приходится пересоздавать сообщение.
        Уже отправленную картинку шлём по file_id — без повторной заливки.
        """
        was_photo = bool(message.photo)
        if photo is None:
            if was_photo:
                await hush(message)
                await message.answer(text, reply_markup=kb)
            else:
                await edit_screen(message, text, reply_markup=kb)
            return

        if key:
            key = f"{key}:{hashlib.sha256(photo).hexdigest()[:16]}"
        cached = deps.cards.file_ids.get(key) if key else None
        media = cached or BufferedInputFile(photo, "card.jpg")
        if was_photo:
            try:
                sent = await message.edit_media(
                    InputMediaPhoto(media=media, caption=text), reply_markup=kb
                )
            except TelegramBadRequest as exc:
                if "message is not modified" in str(exc).lower():
                    return
                raise
        else:
            await hush(message)
            sent = await message.answer_photo(media, caption=text, reply_markup=kb)
        if key and not cached and isinstance(sent, Message) and sent.photo:
            deps.cards.file_ids[key] = sent.photo[-1].file_id

    async def edit_screen(
        message: Message, text: str, reply_markup: Any = None
    ) -> None:
        try:
            if message.photo and len(text) <= 1024:
                await message.edit_caption(caption=text, reply_markup=reply_markup)
            elif message.photo:
                await message.edit_reply_markup(reply_markup=None)
                await message.answer(text, reply_markup=reply_markup)
            else:
                await message.edit_text(text, reply_markup=reply_markup)
        except TelegramBadRequest as exc:
            if "message is not modified" not in str(exc).lower():
                raise

    async def hush(message: Message) -> None:
        """Убирает сообщение пользователя, чтобы диалог оставался чистым."""
        try:
            await message.delete()
        except Exception:  # noqa: BLE001 - право на удаление могут забрать
            pass

    async def show_menu(
        message: Message, tg_id: int, name: str, edit: bool = False
    ) -> None:
        left, credits, sub = deps.quota(tg_id)
        text = ui.greeting(
            name, ui.quota_line(cfg, left, credits, sub, deps.shared(tg_id))
        )
        markup = ui.main_menu(deps.is_admin(tg_id), purchases=True)
        photo = await deps.cards.banner("home")
        if edit:
            await swap(message, text, markup, photo, key="banner:home")
        else:
            await message.answer_photo(
                BufferedInputFile(photo, "signum.jpg"),
                caption=text,
                reply_markup=markup,
            )

    @router.message(
        Command("emoji"), F.from_user.id.in_(cfg.admins), F.chat.type == "private"
    )
    async def emoji_help(message: Message, state: FSMContext) -> None:
        await state.clear()
        await message.answer(
            "Пришлите текст с анимированными эмодзи. Верну тот же текст, заменив их ID.\n\n"
            "Например: Каталог Signum — [выбранный эмодзи].\n"
            "Можно прислать весь текст сразу, с несколькими эмодзи.",
            parse_mode=None,
        )

    @router.edited_message(
        StateFilter(None, Ask.search),
        F.from_user.id.in_(cfg.admins),
        F.chat.type == "private",
        has_custom_emoji,
    )
    @router.message(
        StateFilter(None, Ask.search),
        F.from_user.id.in_(cfg.admins),
        F.chat.type == "private",
        has_custom_emoji,
    )
    async def emoji_extract(message: Message, state: FSMContext) -> None:
        if db.blocked(message.from_user.id):
            return
        text = message.text if message.text is not None else message.caption or ""
        entities = (
            message.entities if message.text is not None else message.caption_entities
        )
        result = replace_custom_emojis(text, entities or [])
        await state.clear()
        if len(result.encode("utf-16-le")) // 2 <= 4096:
            await message.answer(result, parse_mode=None)
        else:
            await message.answer_document(
                BufferedInputFile(result.encode("utf-8"), "emoji-ids.txt"),
                caption="Текст с ID эмодзи — целиком в файле.",
                parse_mode=None,
            )

    # --- старт ------------------------------------------------------------
    @router.message(CommandStart())
    async def start(message: Message, state: FSMContext) -> None:
        await state.clear()
        user = message.from_user
        db.user(user.id, user.username or "", user.full_name)
        if db.blocked(user.id):
            await message.answer("🚫 Доступ к боту закрыт.")
            return
        await show_menu(message, user.id, user.first_name or "друг")

    @router.message(Command("menu"))
    async def menu_cmd(message: Message, state: FSMContext) -> None:
        await state.clear()
        await show_menu(
            message, message.from_user.id, message.from_user.first_name or "друг"
        )

    @router.message(Command("help"))
    async def help_cmd(message: Message) -> None:
        await message.answer(
            ui.how_it_works(
                cfg, deps.chat_limit(message.from_user.id) // (1024 * 1024)
            ),
            reply_markup=ui.back_kb(),
        )

    @router.message(Command("id"))
    async def id_cmd(message: Message) -> None:
        await message.answer(f"🪪 Ваш Telegram ID: <code>{message.from_user.id}</code>")

    @router.callback_query(F.data == "menu")
    async def menu_cb(cq: CallbackQuery, state: FSMContext) -> None:
        await state.clear()
        await cq.answer()
        await show_menu(
            cq.message, cq.from_user.id, cq.from_user.first_name or "друг", edit=True
        )

    @router.callback_query(F.data == "how")
    async def how_cb(cq: CallbackQuery) -> None:
        await cq.answer()
        await edit_screen(
            cq.message,
            ui.how_it_works(cfg, deps.chat_limit(cq.from_user.id) // (1024 * 1024)),
            reply_markup=ui.back_kb(),
        )

    @router.callback_query(F.data == "noop")
    async def noop(cq: CallbackQuery) -> None:
        await cq.answer()

    # --- каталог ----------------------------------------------------------
    @router.callback_query(F.data == "cat")
    async def catalog(cq: CallbackQuery) -> None:
        await cq.answer()
        data = await waiting(cq.message, store.catalog(), "Открываю каталог")
        deps.catalog_cache["apps"] = data.get("apps") or []
        await swap(
            cq.message,
            f"📚 <b>Каталог Signum</b>\n{ui.LINE}\n"
            f"Приложения банков и сервисов, которых\nбольше нет в App Store.\n"
            f"{ui.THIN}\n<i>Выберите раздел:</i>",
            ui.groups_kb(data.get("groups") or []),
            await deps.cards.banner("catalog"),
            key="banner:catalog",
        )

    @router.callback_query(F.data.startswith("cg:"))
    async def catalog_group(cq: CallbackQuery) -> None:
        await cq.answer()
        _, group_id, page_raw = cq.data.split(":")
        data = await waiting(cq.message, store.catalog(), "Открываю раздел")
        apps = data.get("apps") or []
        deps.catalog_cache["apps"] = apps
        title = next(
            (
                g.get("title")
                for g in data.get("groups") or []
                if g.get("id") == group_id
            ),
            group_id,
        )
        picked = [app for app in apps if app.get("category") == group_id]
        chunk, page, total = ui.page_slice(picked, int(page_raw))
        deps.last_list[cq.from_user.id] = f"cg:{group_id}:{page}"
        kb = ui.nav_kb(
            [
                (f"{n}.  {app['title']}", f"ca:{app['id']}")
                for n, app in enumerate(chunk, 1)
            ],
            page,
            total,
            f"cg:{group_id}",
            back_to="cat",
        )
        photo = await waiting(
            cq.message,
            deps.cards.grid(
                f"Каталог · {title}",
                f"Приложений в разделе: {len(picked)}",
                [
                    (app.get("icon_url") or "", app["title"], app.get("version") or "")
                    for app in chunk
                ],
            ),
            "Готовлю список приложений",
        )
        await swap(
            cq.message,
            f"📂 <b>{ui.esc(title)}</b>\n{ui.LINE}\nПриложений в разделе: <b>{len(picked)}</b>",
            kb,
            photo,
            key=f"cg:{group_id}:{page}",
        )

    @router.callback_query(F.data.startswith(("ca:", "car:")))
    async def catalog_app(cq: CallbackQuery) -> None:
        await cq.answer()
        app_id = cq.data.split(":")[1]
        info = await waiting(
            cq.message, store.catalog_app(app_id), "Открываю приложение"
        )
        versions = [v for v in (info.get("versions") or []) if v.get("app_store_id")]
        try:
            account = tool.for_user(cq.from_user.id)
        except NoAccount as exc:
            await edit_screen(
                cq.message,
                ui.failed(
                    str(exc),
                    "Покажу только приложения, которые есть на подключённом Apple ID.",
                ),
                ui.back_kb("me", "Подключить Apple ID"),
            )
            return
        progress = Progress.of(
            cq.message,
            str(info.get("title") or "Приложение"),
            max(1, len({str(v["app_store_id"]) for v in versions})),
        )
        await progress.start("Ищу доступные приложения")

        def report(done: int, total: int) -> None:
            progress.step = done
            progress.percent = done / max(1, total) * 100
            progress.stage = "Ищу доступные приложения"
            progress.hint(f"Проверено {done} из {total}")

        def phase(stage: str) -> None:
            progress.percent = None
            if stage == "prepare":
                progress.stage = "Проверяю Apple ID"
                progress.hint("")
            else:
                progress.stage = "Ищу доступные приложения"
                progress.hint("")

        try:
            scan = await ownership.scan(
                account,
                versions,
                force=cq.data.startswith("car:"),
                progress=report,
                phase=phase,
            )
        except IpatoolError:
            await progress.finish(
                "<b>Не удалось проверить покупки Apple ID</b>\nApple не ответила. Повторите проверку или войдите в Apple ID заново.",
                ui.back_kb(f"car:{app_id}", "Проверить ещё раз"),
            )
            return
        finally:
            progress.stop()
        versions = scan.versions
        warning = (
            f"\nЕщё {scan.unchecked} сборок проверить не удалось. Повторите проверку."
            if scan.unchecked
            else ""
        )
        if not versions:
            kb = InlineKeyboardBuilder()
            kb.button(text="↻ Проверить ещё раз", callback_data=f"car:{app_id}")
            kb.button(text="Мой Apple ID", callback_data="me")
            kb.button(
                text="Назад в каталог", callback_data=f"cg:{info.get('category')}:0"
            )
            kb.adjust(1)
            reason = (
                "Apple не подтвердила доступные сборки. Это не значит, что приложения нет в покупках."
                if scan.unchecked
                else "На этом Apple ID нет подтверждённых сборок в покупках. Если приложение покупалось на другом Apple ID, подключите его."
            )
            await swap(
                cq.message,
                f"<b>{ui.esc(info.get('title'))}</b>\n{ui.LINE}\n{reason}",
                kb.as_markup(),
            )
            return
        # Обложки давно выброшены из App Store, и iTunes про них ничего не знает.
        # Всё, что о них известно, есть здесь — сохраняем, иначе карточка окажется
        # пустой с одним голым номером.
        for version in versions:
            db.app_save(
                {
                    "adam_id": version["app_store_id"],
                    "bundle_id": version.get("bundle_id") or "",
                    "title": version.get("title") or info.get("title") or "",
                    "artist": info.get("title") or "",
                    "icon": version.get("icon_url") or info.get("icon_url") or "",
                    "version": version.get("version") or "",
                    "min_ios": version.get("min_ios") or "",
                    "source": "catalog",
                }
            )
        deps.last_list[cq.from_user.id] = f"ca:{app_id}:0"
        page = int(cq.data.split(":")[2]) if cq.data.count(":") > 1 else 0
        chunk, page, total = ui.page_slice(versions, page)
        deps.last_list[cq.from_user.id] = f"ca:{app_id}:{page}"
        kb = ui.nav_kb(
            [
                (
                    f"{n}.  {'⭐️ ' if v.get('is_latest') else ''}"
                    f"{v.get('title') or info.get('title')}  ·  {v.get('version') or '—'}",
                    f"vsel:{v['app_store_id']}:{v['external_version_id']}"
                    if v.get("external_version_id")
                    else f"app:{v['app_store_id']}",
                )
                for n, v in enumerate(chunk, 1)
            ],
            page,
            total,
            f"ca:{app_id}",
            back_to=f"cg:{info.get('category')}:0",
        )
        kb.inline_keyboard.insert(
            len(chunk), [ui.back(f"car:{app_id}", "↻ Обновить покупки")]
        )
        photo = await waiting(
            cq.message,
            deps.cards.grid(
                str(info.get("title") or ""),
                f"На вашем Apple ID: {len(versions)} сборок",
                [
                    (
                        v.get("icon_url") or info.get("icon_url") or "",
                        v.get("title") or str(info.get("title") or ""),
                        " · ".join(
                            part
                            for part in (
                                v.get("version") or "",
                                f"iOS {v['min_ios']}+" if v.get("min_ios") else "",
                            )
                            if part
                        ),
                    )
                    for v in chunk
                ],
            ),
            "Готовлю список приложений",
        )
        await swap(
            cq.message,
            f"📱 <b>{ui.esc(info.get('title'))}</b>\n{ui.LINE}\n"
            f"Доступно вам: <b>{len(versions)}</b>\n"
            f"{ui.THIN}\n<i>Выберите приложение. ⭐️ — последняя версия.</i>{warning}",
            kb,
            photo,
            key=f"ca:{account['id']}:{app_id}:{page}:{hashlib.sha256(photo).hexdigest()[:16]}",
        )

    # --- поиск ------------------------------------------------------------
    @router.callback_query(F.data.in_({"srch", "srch:developer"}))
    async def search_prompt(cq: CallbackQuery, state: FSMContext) -> None:
        await cq.answer()
        await state.set_state(Ask.search)
        developer_mode = cq.data == "srch:developer"
        await state.update_data(search_mode="developer" if developer_mode else "app")
        search_kb = InlineKeyboardBuilder()
        search_kb.button(
            text="По названию приложения" if developer_mode else "По разработчику",
            callback_data="srch" if developer_mode else "srch:developer",
        )
        search_kb.button(text="Главное меню", callback_data="menu")
        search_kb.adjust(1)
        await swap(
            cq.message,
            (
                f"🔎 <b>Поиск по разработчику</b>\n{ui.LINE}\nПришлите имя разработчика или название компании."
                if developer_mode
                else f"🔎 <b>Найти приложение</b>\n{ui.LINE}\n"
                f"Пришлите одним сообщением:\n"
                f"• название приложения\n"
                f"• идентификатор приложения, если знаете его\n"
                f"• ссылку вида <code>apps.apple.com/…/id123456</code>"
            ),
            kb=search_kb.as_markup(),
            photo=await deps.cards.banner("search"),
            key="banner:search",
        )

    @router.message(Ask.search)
    async def search_run(message: Message, state: FSMContext) -> None:
        developer_mode = (await state.get_data()).get("search_mode") == "developer"
        await state.clear()
        term = (message.text or "").strip()
        if not term:
            return
        adam = _adam_from_text(term)
        if adam and not developer_mode:
            await _send_card(message, adam, message.from_user.id)
            return
        note = await message.answer(f"🌘 <b>Ищу «{ui.esc(term)}»…</b>")
        progress = Progress.of(note, term)
        await progress.start(
            "Ищу разработчиков" if developer_mode else "Ищу приложения"
        )
        try:
            results = (
                await store.developers(term)
                if developer_mode
                else await store.search(term, limit=100)
            )
        except Exception:  # noqa: BLE001 - причину должен видеть человек
            await progress.finish(
                ui.failed("Поиск временно недоступен. Попробуйте ещё раз."),
                ui.back_kb(),
            )
            return
        finally:
            progress.stop()
        if developer_mode:
            deps.developer_cache[message.from_user.id] = results
            await _show_developers(note, message.from_user.id, 0)
            return
        if not results:
            await note.edit_text(
                f"😔 <b>Ничего не нашлось</b>\n{ui.LINE}\n"
                f"По запросу «{ui.esc(term)}» пусто.\n<i>Попробуйте другое название.</i>",
                reply_markup=ui.back_kb(),
            )
            return
        deps.search_cache[message.from_user.id] = results
        await _show_search(note, message.from_user.id, 0, term)

    async def _show_developers(message, tg_id, page):
        results = deps.developer_cache.get(tg_id) or []
        chunk, page, total = ui.page_slice(results, page)
        kb = ui.nav_kb(
            [
                (f"{row['name']} · {row['apps']}", f"dapps:{row['artist_id']}")
                for row in chunk
            ],
            page,
            total,
            "devpage",
            back_to="srch:developer",
        )
        await edit_screen(
            message,
            "<b>Разработчики</b>\nВыберите, чьи приложения показать."
            if results
            else "<b>Разработчик не найден</b>\nПопробуйте другое имя.",
            kb,
        )

    @router.callback_query(F.data.startswith("devpage:"))
    async def developer_page(cq: CallbackQuery):
        await cq.answer()
        await _show_developers(cq.message, cq.from_user.id, int(cq.data.split(":")[1]))

    @router.callback_query(F.data.startswith("dapps:"))
    async def developer_apps(cq: CallbackQuery):
        await cq.answer()
        artist = cq.data.split(":", 1)[1]
        if not re.fullmatch(r"[0-9]{1,20}", artist):
            return
        results = await waiting(
            cq.message, store.developer_apps(artist), "Ищу приложения разработчика"
        )
        for item in results:
            db.app_save(item)
        deps.search_cache[cq.from_user.id] = results
        await _show_search(cq.message, cq.from_user.id, 0, "Приложения разработчика")

    async def _show_search(
        message: Message, tg_id: int, page: int, term: str = ""
    ) -> None:
        results = [
            app for app in (deps.search_cache.get(tg_id) or []) if app.get("adam_id")
        ]
        chunk, page, total = ui.page_slice(results, page)
        kb = ui.nav_kb(
            [
                (f"{n}.  {app['title']}", f"app:{app['adam_id']}")
                for n, app in enumerate(chunk, 1)
            ],
            page,
            total,
            "sp",
            back_to="menu",
        )
        query = term or deps.search_terms.get(tg_id, "")
        deps.search_terms[tg_id] = query
        deps.last_list[tg_id] = f"sp:{page}"
        photo = await waiting(
            message,
            deps.cards.grid(
                f"Поиск · {query}" if query else "Поиск",
                f"Нашёл приложений: {len(results)}",
                [
                    (
                        app.get("icon") or "",
                        app.get("title") or "",
                        " · ".join(
                            part
                            for part in (
                                app.get("version") or "",
                                ota.human_size(app.get("size") or 0),
                            )
                            if part
                        ),
                    )
                    for app in chunk
                ],
            ),
            "Готовлю список приложений",
        )
        head = "🔎 <b>Результаты</b>" + (f" · {ui.esc(query)}" if query else "")
        await swap(
            message,
            f"{head}\n{ui.LINE}\nНашёл приложений: <b>{len(results)}</b>",
            kb,
            photo,
            key=f"sp:{tg_id}:{query}:{page}",
        )

    @router.callback_query(F.data.startswith("sp:"))
    async def search_page(cq: CallbackQuery) -> None:
        await cq.answer()
        await _show_search(cq.message, cq.from_user.id, int(cq.data.split(":")[1]))

    # --- карточка приложения ---------------------------------------------
    async def _send_card(
        message: Message,
        adam_id: str,
        tg_id: int,
        back_to: str = "menu",
        edit: bool = False,
    ) -> None:
        cached = db.app(adam_id)
        app: dict[str, Any] = dict(cached) if cached else {}
        # Каталог знает обложки, iTunes — размер и издателя. Берём и то и другое:
        # что пришло свежим и непустым, то и побеждает.
        found = await waiting(
            message, store.lookup(adam_id), "Открываю приложение", new_message=not edit
        )
        for key, value in (found or {}).items():
            if value or not app.get(key):
                app[key] = value
        app["adam_id"] = str(adam_id)
        if not app.get("title"):
            app["title"] = str(adam_id)
        db.app_save(app)
        limit = deps.size_limit(tg_id)
        size = int(app.get("size") or 0)
        extra = ""
        if size and size > limit:
            extra = (
                f"\n🚫 <i>Больше вашего лимита ({ota.human_size(limit)}).</i>"
                if db.sub_active(tg_id)
                else f"\n🚫 <i>Больше лимита {ota.human_size(limit)} — нужна подписка.</i>"
            )
        text = ui.app_card(
            app, extra=extra or "\n<i>Выберите, как получить приложение:</i>"
        )
        kb = ui.app_kb(str(adam_id), back_to=deps.last_list.get(tg_id, back_to))
        lines = [
            f"Версия  ·  {app.get('version') or '—'}",
            f"Нужна iOS  ·  {app['min_ios']}+" if app.get("min_ios") else "",
            f"Размер  ·  {ota.human_size(size)}" if size else "",
        ]
        photo = await waiting(
            message,
            deps.cards.hero(
                {
                    "title": str(app.get("title") or ""),
                    "artist": str(app.get("artist") or ""),
                    "icon": str(app.get("icon") or ""),
                    "header": "Приложение",
                    "subheader": "Выберите, как его получить",
                },
                [line for line in lines if line],
            ),
            "Готовлю карточку приложения",
            new_message=not edit,
        )
        key = f"app:{adam_id}"
        if edit:
            await swap(message, text, kb, photo, key=key)
            return
        key = f"{key}:{hashlib.sha256(photo).hexdigest()[:16]}"
        cached_id = deps.cards.file_ids.get(key)
        sent = await message.answer_photo(
            cached_id or BufferedInputFile(photo, "card.jpg"),
            caption=text,
            reply_markup=kb,
        )
        if not cached_id and sent.photo:
            deps.cards.file_ids[key] = sent.photo[-1].file_id

    @router.callback_query(F.data.startswith("app:"))
    async def app_card(cq: CallbackQuery) -> None:
        await cq.answer()
        await _send_card(cq.message, cq.data.split(":")[1], cq.from_user.id, edit=True)

    @router.callback_query(F.data.startswith("ver:"))
    async def app_versions(cq: CallbackQuery) -> None:
        await cq.answer()
        _, adam_id, page_raw = cq.data.split(":")
        if not adam_id.isdigit() or not page_raw.isdigit():
            return
        try:
            account = tool.for_user(cq.from_user.id)
        except NoAccount:
            account = None
        if account is None:
            await edit_screen(
                cq.message,
                "<b>Подключите Apple ID</b>\nСписок старых сборок запрашивается с вашего аккаунта.",
                ui.back_kb("me", "Подключить Apple ID"),
            )
            return
        key = f"{ownership.session_revision(account)}:{adam_id}"
        try:
            versions = deps.version_cache.get(key)
            if versions is None or int(page_raw) == 0:
                versions = await waiting(
                    cq.message,
                    tool.versions(account, adam_id),
                    "Ищу другие версии",
                )
                versions = sorted(set(versions), key=int, reverse=True)
                deps.version_cache[key] = versions
        except IpatoolError as exc:
            logging.getLogger("signum-bot.versions").warning(
                "Apple versions failed for app %s (%s)",
                adam_id,
                "app_unavailable" if exc.unavailable else "apple_request_failed",
            )
            if exc.unavailable:
                kb = InlineKeyboardBuilder()
                kb.button(text="Повторить", callback_data=f"ver:{adam_id}:0")
                kb.button(text="Назад", callback_data=f"app:{adam_id}")
                kb.adjust(1)
                await edit_screen(
                    cq.message,
                    f"<b>Не удалось получить версии</b>\n{ui.LINE}\nApple не прислала список версий. Попробуйте ещё раз чуть позже.",
                    kb.as_markup(),
                )
                return
            await edit_screen(
                cq.message, ui.failed(str(exc)), ui.back_kb(f"app:{adam_id}")
            )
            return
        chunk, page, total = ui.page_slice(versions, int(page_raw))
        missing = [
            version for version in chunk if (key, version) not in deps.version_labels
        ]
        if missing:

            async def load_labels():
                try:
                    async with tool.probe_homes(account, min(4, len(missing))) as homes:

                        async def worker(index, home):
                            for version in missing[index :: len(homes)]:
                                try:
                                    metadata = await tool.version_metadata(
                                        account, adam_id, version, home_override=home
                                    )
                                except IpatoolError:
                                    continue
                                deps.version_labels[(key, version)] = metadata[
                                    "version"
                                ]

                        await asyncio.gather(
                            *(worker(index, home) for index, home in enumerate(homes))
                        )
                except IpatoolError:
                    pass

            await waiting(cq.message, load_labels(), "Проверяю версии")
        text = "<b>Другие версии</b>\n\n" + (
            "Новые версии — в начале списка. Выберите нужную."
            if versions
            else "Apple не вернул список старых сборок."
        )
        kb = ui.nav_kb(
            [
                (
                    f"{page * ui.PAGE_SIZE + index + 1}. {deps.version_labels.get((key, version)) or f'Сборка {version}'}",
                    f"vsel:{adam_id}:{version}",
                )
                for index, version in enumerate(chunk)
            ],
            page,
            total,
            f"ver:{adam_id}",
            back_to=f"app:{adam_id}",
        )
        await edit_screen(cq.message, text, kb)

    @router.callback_query(F.data.startswith("vsel:"))
    async def select_version(cq: CallbackQuery) -> None:
        await cq.answer()
        _, adam_id, external = cq.data.split(":")
        if not adam_id.isdigit() or not external.isdigit():
            return
        try:
            account = tool.for_user(cq.from_user.id)
            metadata = await waiting(
                cq.message,
                tool.version_metadata(account, adam_id, external),
                "Открываю версию",
            )
        except IpatoolError as exc:
            await edit_screen(
                cq.message, ui.failed(str(exc)), ui.back_kb(f"ver:{adam_id}:0")
            )
            return
        cached = db.app(adam_id)
        app = dict(cached) if cached else {"title": adam_id}
        app.update(version=metadata["version"], min_ios="", size=0)
        photo = await waiting(
            cq.message,
            deps.cards.hero(
                {
                    "title": str(app.get("title") or ""),
                    "artist": str(app.get("artist") or ""),
                    "icon": str(app.get("icon") or ""),
                    "header": "Приложение",
                    "subheader": "Выбрана предыдущая версия",
                },
                [f"Версия  ·  {metadata['version']}"],
            ),
            "Открываю версию",
        )
        await swap(
            cq.message,
            ui.app_card(app, extra="\n<b>Выбрана одна из предыдущих версий</b>"),
            ui.app_kb(adam_id, version=external, back_to=f"ver:{adam_id}:0"),
            photo,
            key=f"app:{adam_id}:version:{external}",
        )

    # --- выдача -----------------------------------------------------------
    @router.callback_query(F.data.startswith(("dl:", "ota:")))
    async def deliver(cq: CallbackQuery) -> None:
        kind, adam_id, *rest = cq.data.split(":")
        kind = "ipa" if kind == "dl" else "ota"
        external = rest[0] if rest else None
        tg_id = cq.from_user.id
        db.user(tg_id, cq.from_user.username or "", cq.from_user.full_name)
        if db.blocked(tg_id):
            await cq.answer("Доступ к боту закрыт", show_alert=True)
            return

        if db.account_of(tg_id) is None:
            await cq.answer("Сначала подключите свой Apple ID", show_alert=True)
            await cq.message.answer(
                ui.my_account(None), reply_markup=ui.my_account_kb(None)
            )
            return

        paid_with = deps.may_install(tg_id)
        if paid_with is None:
            await cq.answer(
                "Бесплатные установки на этой неделе закончились", show_alert=True
            )
            left, credits, sub = deps.quota(tg_id)
            text, kb = ui.paywall(cfg, left, credits, sub, deps.shared(tg_id))
            await cq.message.answer(text, reply_markup=kb)
            return

        # Известный размер отсекаем до скачивания: незачем тянуть три гигабайта,
        # чтобы потом сказать «не поместилось».
        cached = db.app(adam_id)
        limit = deps.limit_for(tg_id, kind)
        known = int(cached["size"]) if cached and not external else 0
        if (
            paid_with == "free"
            and known > cfg.max_bytes(False)
            and db.credits(tg_id) > 0
        ):
            paid_with = "credit"
        if known and known > limit:
            await cq.answer("Слишком большое приложение", show_alert=True)
            await cq.message.answer(
                ui.too_big(known, limit, bool(db.sub_active(tg_id))),
                reply_markup=ui.back_kb("buy", "💎 Подписка"),
            )
            return

        await cq.answer("Загрузка началась")
        title = (cached["title"] if cached else "").strip()
        if not title or title == adam_id:
            # В карточку могли прийти по кнопке «Повторить» — метаданных ещё нет.
            found = await store.lookup(adam_id)
            if found:
                db.app_save({**found, "adam_id": adam_id})
                title = found.get("title") or adam_id
        title = title or adam_id
        note = await cq.message.answer(
            f"🌑 <b>Начинаю…</b>\n{ui.LINE}\n📱 <b>{ui.esc(title)}</b>"
        )
        job_id = f"{tg_id}:{note.message_id}"
        deps.running[job_id] = Job(tg_id=tg_id, title=title)
        task = asyncio.create_task(
            _job(cq.bot, note, tg_id, adam_id, title, kind, external, paid_with, job_id)
        )
        deps.job_tasks.add(task)
        task.add_done_callback(deps.job_tasks.discard)

    @router.callback_query(F.data.startswith("stop:"))
    async def stop_job(cq: CallbackQuery) -> None:
        job = deps.running.get(cq.data.split(":", 1)[1])
        if job is None:
            await cq.answer("Эта загрузка уже закончилась", show_alert=True)
            return
        if job.tg_id != cq.from_user.id:
            await cq.answer("Это не ваша загрузка", show_alert=True)
            return
        if job.stop():
            await cq.answer("Останавливаю…")
        else:
            await cq.answer(
                f"Уже скачано больше половины ({job.percent:.0f}%) — проще дождаться.",
                show_alert=True,
            )

    async def _job(
        bot: Bot,
        note: Message,
        tg_id: int,
        adam_id: str,
        title: str,
        kind: str,
        external: str | None,
        paid_with: str,
        job_id: str,
    ) -> None:
        progress = Progress.of(note, title, 5 if kind == "ota" else 3)
        known = db.app(adam_id)
        cached_size = int(known["size"]) if known else 0
        job = deps.running.get(job_id) or Job(tg_id=tg_id, title=title)
        deps.running[job_id] = job
        account = None
        out_dir = None
        try:
            from .production import claim_download

            claim_download(cfg, db, tg_id, job_id, paid_with)
            reserve_storage(
                cfg, db, "bot:" + job_id, cfg.max_bytes(paid_with != "free") * 2
            )
            await progress.start("Проверяю Apple ID")
            account = tool.for_user(tg_id)
            await tool.revalidate(account)

            def show(percent: float) -> None:
                job.percent = percent
                progress.percent = percent
                if percent >= CANCEL_LIMIT and progress.markup is not None:
                    progress.markup = None  # отменять уже поздно, убираем кнопку

            def tick(percent: float) -> None:
                job.from_tool = True
                show(percent)

            expected = int(cached_size or 0)
            out_dir = tool.home(int(account["id"])) / "ipa" / secrets.token_hex(4)
            out_dir.mkdir(parents=True, exist_ok=True)

            async def watch() -> None:
                """Считает прогресс по размеру файла на диске.

                Полосу загрузки ipatool в JSON-режиме не печатает, а ждать
                несколько минут перед неподвижным нулём невыносимо. Файл растёт
                на глазах — по нему и считаем.
                """
                while True:
                    await asyncio.sleep(2)
                    if job.from_tool:
                        continue
                    done = sum(
                        p.stat().st_size for p in out_dir.rglob("*") if p.is_file()
                    )
                    if not done:
                        continue
                    if expected > 0:
                        show(min(99.0, done / expected * 100))
                        progress.hint(
                            f"{ota.human_size(done)} из {ota.human_size(expected)}"
                        )
                    else:
                        # размер неизвестен — показываем хотя бы скачанное
                        progress.percent = None
                        progress.hint(f"Скачано {ota.human_size(done)}")

            await progress.to("Скачиваю приложение", percent=0.0)
            progress.markup = ui.cancel_kb_job(job_id)
            watcher = asyncio.create_task(watch())
            try:
                ipa = await tool.download(
                    account,
                    adam_id,
                    external_version_id=external,
                    out_dir=out_dir,
                    on_progress=tick,
                    on_start=lambda proc: setattr(job, "proc", proc),
                )
            finally:
                watcher.cancel()
            progress.markup = None
            progress.percent = None

            if (
                paid_with == "free"
                and ipa.stat().st_size > cfg.max_bytes(False)
                and db.credits(tg_id) > 0
            ):
                paid_with = "credit"
            limit = deps.limit_for(tg_id, kind)
            if paid_with == "free":
                limit = min(limit, cfg.max_bytes(False))
            if ipa.stat().st_size > limit:
                size = ipa.stat().st_size
                if not external:
                    db.app_save({**dict(known or {}), "adam_id": adam_id, "size": size})
                shutil.rmtree(ipa.parent, ignore_errors=True)
                db.history_add(
                    tg_id, adam_id, title, kind, int(account["id"]), "toobig"
                )
                await progress.finish(
                    ui.too_big(size, limit, bool(db.sub_active(tg_id))),
                    ui.back_kb("buy", "💎 Подписка"),
                )
                return

            # Настоящий размер узнаём только сейчас: в следующий раз полоса
            # загрузки для этого приложения пойдёт с первой секунды.
            real_size = ipa.stat().st_size
            if known and not external and int(known["size"] or 0) != real_size:
                db.app_save({**dict(known), "size": real_size})

            token = secrets.token_urlsafe(12)
            target_dir = cfg.files_dir / token
            target_dir.mkdir(parents=True, exist_ok=True)
            target = target_dir / ipa.name
            shutil.move(str(ipa), target)
            shutil.rmtree(ipa.parent, ignore_errors=True)

            signed = False
            if kind == "ota":
                # Архив от ipatool iPhone скачает, но ставить откажется —
                # пересобираем его перед выдачей.
                await progress.to("Готовлю приложение к установке")
                await asyncio.to_thread(ota.repack, target)
                await progress.to("Готовлю приложение к установке")
                try:
                    signed = await ota.sign(cfg, target)
                except Exception as exc:  # noqa: BLE001 - подпись необязательна
                    logging.getLogger(__name__).warning(
                        "Optional signing failed: %s", type(exc).__name__
                    )

            meta = db.app(adam_id)
            facts = ota.ipa_facts(target)
            db.file_add(
                token,
                adam_id=adam_id,
                account_id=int(account["id"]),
                tg_id=tg_id,
                name=target.name,
                path=str(target),
                size=target.stat().st_size,
                bundle_id=facts["bundle_id"]
                or (meta["bundle_id"] if meta else "")
                or "",
                version=facts["version"] or (meta["version"] if meta else "") or "",
                title=title,
                signed=int(signed),
                encrypted=int(facts["encrypted"]),
                paid_with=paid_with,
                ttl_hours=cfg.link_hours(paid_with),
                expires_at=ota.expiry(cfg, paid_with),
            )
            await progress.to("Готовлю ссылку")
            if cfg.bridge_enabled:
                from .bridge import publish_file

                await progress.to("Сохраняю файл", "Готовлю ссылку на скачивание.")
                await publish_file(cfg, db, token)
            delivered = await _hand_over(
                bot, progress, tg_id, token, title, kind, str(account["email"])
            )
            if delivered and cfg.bridge_enabled:
                shutil.rmtree(target_dir, ignore_errors=True)

            db.history_add(
                tg_id,
                adam_id,
                title,
                kind,
                int(account["id"]),
                "ok" if delivered else "fail",
                paid_with if delivered else "",
            )
            if delivered and paid_with == "credit":
                db.add_credits(tg_id, -1)
        except NoAccount as exc:
            await progress.finish(
                ui.failed(str(exc)), ui.back_kb("me", "🍏 Мой Apple ID")
            )
        except IpatoolError as exc:
            if job.cancelled:
                db.history_add(
                    tg_id,
                    adam_id,
                    title,
                    kind,
                    int(account["id"]) if account else 0,
                    "cancel",
                )
                await progress.finish(
                    ui.cancelled(title), ui.back_kb("menu", "◀︎ В меню")
                )
                return
            db.history_add(
                tg_id,
                adam_id,
                title,
                kind,
                int(account["id"]) if account else 0,
                "fail",
            )
            hint = ""
            if exc.unavailable:
                hint = "Приложение недоступно в стране вашего Apple ID."
            elif exc.not_licensed:
                hint = "Приложения нет в покупках этого Apple ID."
            elif exc.needs_login and account is not None:
                db.account_status(int(account["id"]), "error", str(exc)[:180])
                hint = "Откройте «Мой Apple ID» и войдите заново."
            await progress.finish(ui.failed(str(exc), hint), ui.back_kb())
        except Exception as exc:  # noqa: BLE001 - что угодно, но человек должен узнать
            db.history_add(
                tg_id,
                adam_id,
                title,
                kind,
                int(account["id"]) if account else 0,
                "fail",
            )
            await progress.finish(ui.failed(str(exc)), ui.back_kb())
        finally:
            if out_dir is not None:
                resolved = out_dir.resolve()
                if resolved.parent.name == "ipa" and resolved.is_relative_to(
                    cfg.accounts_dir.resolve()
                ):
                    await asyncio.to_thread(shutil.rmtree, resolved, True)
            progress.markup = None
            progress.stop()
            deps.running.pop(job_id, None)
            db._run("delete from download_claims where id=?", (job_id,))
            release_storage(db, "bot:" + job_id)

    async def _hand_over(
        bot: Bot,
        progress: Progress,
        tg_id: int,
        token: str,
        title: str,
        kind: str,
        email: str,
    ) -> bool:
        """Отдаёт готовое приложение. Ссылка — только для установки на iPhone."""
        row = db.file(token)
        assert row is not None
        size = int(row["size"])

        if kind == "ipa" and size > deps.sender.limit(size):
            if not cfg.public_base_url:
                await progress.finish(
                    ui.failed("Этот файл слишком большой для отправки в чат."),
                    ui.back_kb(),
                )
                return False
            kb = InlineKeyboardBuilder()
            kb.button(
                text="📥 Скачать приложение", url=f"{cfg.public_base_url}/f/{token}.ipa"
            )
            kb.button(text="◀︎ В меню", callback_data="menu")
            kb.adjust(1)
            await progress.finish(
                f"✅ <b>Приложение готово</b>\n\n<b>{ui.esc(title)}</b>\n{ota.human_size(size)}\n\n"
                f"Нажмите кнопку, чтобы скачать файл целиком.\n<i>Ссылка действует {row['ttl_hours']} ч.</i>",
                kb.as_markup(),
            )
            return True

        if kind == "ota":
            if not cfg.public_base_url:
                await progress.finish(
                    ui.failed(
                        "Установка сейчас недоступна.",
                        "Вы можете скачать файл через «Скачать .ipa».",
                    ),
                    ui.back_kb(),
                )
                return False
            kb = InlineKeyboardBuilder()
            kb.button(
                text="Установить",
                url=f"{cfg.public_base_url}/i/{token}",
            )
            kb.button(text="◀︎ В меню", callback_data="menu")
            kb.adjust(1)
            await progress.finish(
                ui.ready_ota(
                    title, size, int(row["ttl_hours"]), email, bool(row["encrypted"])
                ),
                kb.as_markup(),
            )
            return True

        caption = (
            f"📱 <b>{ui.esc(title)}</b>\n"
            f"💾 {ota.human_size(size)}\n{ui.THIN}\n"
            f"<i>Ставится через Signum, AltStore или Sideloadly.</i>"
        )
        try:
            await progress.to("Отправляю файл", ota.human_size(size))
            async with ChatActionSender.upload_document(bot=bot, chat_id=tg_id):
                if size > cfg.upload_limit:
                    # Bot API дальше 50 МБ не пускает — заливаем по MTProto.
                    await deps.sender.send(
                        tg_id, Path(row["path"]), caption, on_progress=progress.hint
                    )
                else:
                    await bot.send_document(
                        tg_id,
                        FSInputFile(row["path"], filename=row["name"]),
                        caption=caption,
                    )
        except Exception as exc:  # noqa: BLE001 - файл остаётся на диске, можно повторить
            kb = InlineKeyboardBuilder()
            kb.button(text="🔁 Повторить", callback_data=f"dl:{row['adam_id']}")
            kb.button(text="◀︎ В меню", callback_data="menu")
            kb.adjust(1)
            await progress.finish(ui.send_failed(title, str(exc)), kb.as_markup())
            return False

        # Файл дошёл до человека — на диске он больше не нужен.
        shutil.rmtree(Path(row["path"]).parent, ignore_errors=True)
        if not row["remote"]:
            db.file_delete(token)
        await progress.finish(
            f"✅ <b>Файл отправлен</b>\n{ui.LINE}\n📱 <b>{ui.esc(title)}</b>\n💾 {ota.human_size(size)}",
            ui.back_kb("menu", "◀︎ В меню"),
        )
        return True

    # --- история ----------------------------------------------------------
    @router.callback_query(F.data == "hist")
    async def history(cq: CallbackQuery) -> None:
        await cq.answer()
        rows = db.history(cq.from_user.id, limit=10)
        kb = InlineKeyboardBuilder()
        for item in rows[:8]:
            if item["adam_id"]:
                kb.button(
                    text=f"🔁 {item['title']}", callback_data=f"app:{item['adam_id']}"
                )
        kb.adjust(1)
        tail = InlineKeyboardBuilder()
        tail.button(text="◀︎ Назад", callback_data="menu")
        kb.attach(tail)
        await edit_screen(
            cq.message, ui.history_text(rows), reply_markup=kb.as_markup()
        )

    # --- оплата -----------------------------------------------------------
    async def show_paywall(message: Message, tg_id: int, edit: bool) -> None:
        left, credits, sub = deps.quota(tg_id)
        text, kb = ui.paywall(cfg, left, credits, sub, deps.shared(tg_id))
        if edit:
            await edit_screen(message, text, reply_markup=kb)
        else:
            await message.answer(text, reply_markup=kb)

    @router.message(Command("buy"))
    async def buy_cmd(message: Message) -> None:
        await show_paywall(message, message.from_user.id, edit=False)

    @router.callback_query(F.data == "buy")
    async def buy_cb(cq: CallbackQuery) -> None:
        await cq.answer()
        await show_paywall(cq.message, cq.from_user.id, edit=True)

    @router.callback_query(F.data.startswith("pay:"))
    async def pay(cq: CallbackQuery) -> None:
        if (
            cfg.production
            and cq.from_user.id not in cfg.admins
            and (
                cfg.payment_mode != "yookassa"
                or not deps.payments
                or deps.payments.test
            )
        ):
            await cq.answer(
                "Оплата скоро появится. Бесплатные загрузки доступны.", show_alert=True
            )
            return
        plan = cq.data.split(":", 1)[1]
        if plan not in cfg.prices or db.blocked(cq.from_user.id):
            await cq.answer("Тариф недоступен", show_alert=True)
            return
        if cfg.payment_mode != "demo" and not (deps.payments and deps.payments.ready):
            await cq.answer("Оплата временно недоступна", show_alert=True)
            return
        db.user(cq.from_user.id, cq.from_user.username or "", cq.from_user.full_name)
        await cq.answer()
        try:
            identifier = create_order(cfg, db, cq.from_user.id, plan)
        except ValueError:
            await cq.message.answer(
                "Сначала подключите API, затем можно пополнить загрузки."
            )
            return
        if cfg.payment_mode == "yookassa":
            await edit_screen(
                cq.message,
                "⏳ <b>Открываю оплату</b>",
            )
            try:
                url = await waiting(
                    cq.message,
                    deps.payments.checkout(identifier, cq.from_user.id),
                    "Открываю оплату",
                )
            except PaymentError as exc:
                logging.getLogger("signum-bot.payments").warning(
                    "Checkout failed for plan %s (%s)", plan, str(exc)
                )
                await edit_screen(
                    cq.message,
                    "Не удалось открыть оплату. Попробуйте чуть позже.",
                    reply_markup=ui.back_kb("api" if plan.startswith("api") else "buy"),
                )
                return
            kb = InlineKeyboardBuilder()
            kb.button(text=f"Оплатить {cfg.price_label(plan)}", url=url)
            kb.button(text="Проверить оплату", callback_data=f"paycheck:{identifier}")
            kb.button(
                text="◀︎ Назад", callback_data="api" if plan.startswith("api") else "buy"
            )
            kb.adjust(1)
            note = (
                "Проверка оплаты. Деньги не списываются."
                if deps.payments.test
                else "После оплаты покупка появится автоматически."
            )
            await edit_screen(
                cq.message,
                f"<b>{ui.PLAN_TITLES[plan]}</b>\n{ui.LINE}\nК оплате: <b>{cfg.price_label(plan)}</b>\n\n{note}\n<i>Без автопродления и сохранения карты.</i>",
                reply_markup=kb.as_markup(),
            )
            return
        kb = InlineKeyboardBuilder()
        kb.button(
            text="Проверить покупку",
            callback_data=f"demo:{identifier}",
        )
        kb.button(
            text="◀︎ Назад", callback_data="api" if plan.startswith("api") else "buy"
        )
        kb.adjust(1)
        await edit_screen(
            cq.message,
            f"<b>{ui.PLAN_TITLES[plan]}</b>\n{ui.LINE}\n"
            f"К оплате: <b>{cfg.price_label(plan)}</b>\n\n"
            "<b>Проверка покупки</b>\nДеньги не списываются.\n"
            "<i>Автопродления и привязки карты нет.</i>",
            reply_markup=kb.as_markup(),
        )

    @router.callback_query(F.data.startswith("paycheck:"))
    async def check_pay(cq: CallbackQuery) -> None:
        await cq.answer()
        if not deps.payments or not deps.payments.ready:
            return
        identifier = cq.data.split(":", 1)[1]
        try:
            status = await waiting(
                cq.message,
                deps.payments.verify(identifier, cq.from_user.id),
                "Проверяю оплату",
                new_message=True,
            )
        except PaymentError:
            await cq.message.answer(
                "Не удалось проверить оплату. Нажмите «Проверить оплату» чуть позже."
            )
            return
        if status != "complete":
            await cq.message.answer(
                "Оплата отменена. Выберите тариф заново."
                if status == "canceled"
                else "Оплата пока не подтверждена. Завершите её по кнопке выше."
            )
            return
        order = db.one("select plan,payment_test from orders where id=?", (identifier,))
        if order["plan"].startswith("api"):
            await show_api(cq.message, cq.from_user.id)
        else:
            note = (
                "\n\n<i>Тестовая оплата: настоящие деньги не списывались.</i>"
                if order["payment_test"]
                else ""
            )
            await edit_screen(
                cq.message,
                f"✅ <b>Покупка активирована</b>\n{ui.LINE}\n{ui.PLAN_TITLES[order['plan']]}{note}",
                reply_markup=ui.main_menu(
                    deps.is_admin(cq.from_user.id), purchases=True
                ),
            )

    @router.callback_query(F.data.startswith("demo:"))
    async def demo_pay(cq: CallbackQuery) -> None:
        if cfg.production and cq.from_user.id not in cfg.admins:
            await cq.answer("Оплата недоступна", show_alert=True)
            return
        if cfg.payment_mode != "demo":
            await cq.answer("Оплата недоступна", show_alert=True)
            return
        identifier = cq.data.split(":", 1)[1]
        result = activate_demo(db, cq.from_user.id, identifier)
        if result != "activated":
            messages = {
                "complete": "Этот заказ уже активирован",
                "expired": "Заказ устарел — выберите тариф заново",
            }
            await cq.answer(messages.get(result, "Заказ недоступен"), show_alert=True)
            return
        order = db.one("select plan from orders where id=?", (identifier,))
        await cq.answer("Тариф активирован")
        if order["plan"].startswith("api"):
            await show_api(cq.message, cq.from_user.id)
        else:
            await edit_screen(
                cq.message,
                f"✅ <b>Тариф активирован</b>\n{ui.LINE}\n"
                f"{ui.PLAN_TITLES[order['plan']]}\n\n<i>Проверка покупки завершена. Деньги не списывались.</i>",
                reply_markup=ui.main_menu(
                    deps.is_admin(cq.from_user.id), purchases=True
                ),
            )

    async def show_api(message: Message, tg_id: int, edit: bool = True) -> None:
        user = db.one(
            "select api_until,api_remaining,api_extra_remaining from users where tg_id=?",
            (tg_id,),
        )
        active = user and user["api_until"] > ota.now()
        text = "<b>Signum API</b>\n"
        kb = InlineKeyboardBuilder()
        if active:
            remaining = user["api_remaining"] + user["api_extra_remaining"]
            remaining_label = f"{remaining:,}".replace(",", " ")
            text += (
                f"Доступ до <b>{ui.when(user['api_until'])}</b>\n\n"
                f"Осталось загрузок · <b>{remaining_label}</b>\n"
            )
            if user["api_extra_remaining"]:
                text += f"Докуплено · <b>{user['api_extra_remaining']:,}</b>\n".replace(",", " ")
            kb.button(text="🔐 Мои ключи", callback_data=f"kp:{tg_id}:0")
            kb.button(text="Пополнить загрузки", callback_data="apipacks")
        else:
            if user and user["api_extra_remaining"]:
                extra_label = f"{user['api_extra_remaining']:,}".replace(",", " ")
                text += (
                    f"\nСохранено загрузок · <b>{extra_label}</b>\n"
                    "Продлите доступ, чтобы использовать их.\n\n"
                )
        downloads_label = f"{cfg.api_downloads:,}".replace(",", " ")
        text += (
            f"\n<b>{downloads_label} загрузок</b> · <b>{cfg.price_label('api')}</b> / 30 дней\n"
            f"Файлы до <b>{cfg.max_mb_sub // 1024} ГБ</b> · ссылки на <b>{cfg.link_ttl_paid_hours} ч.</b>\n\n"
            "<b>Возможности</b>\n"
            "• Apple ID: вход без бота, код при необходимости, несколько аккаунтов, восстановление по ID и удаление.\n"
            "• Каталог и поиск приложений по названию и разработчику.\n"
            "• Данные приложения и список доступных версий.\n"
            "• Скачивание .ipa или установка на iPhone — с выбранного аккаунта и нужной версии.\n"
            "• Ссылка на файл, страница и прямая ссылка на установку.\n"
            "• Проверка статуса, отмена загрузки и удаление файла.\n"
            "• Создание ключей, права доступа, лимиты, приостановка и отзыв.\n"
            "• Баланс, продление, покупка загрузок и проверка оплаты.\n\n"
            "Каждый ключ имеет доступ только к своим Apple ID.\n"
            "<i>Доступ к API приобретается отдельно.</i>"
        )
        kb.button(
            text=f"{'Продлить' if active else 'Подключить'} · {cfg.price_label('api')}",
            callback_data="pay:api",
        )
        if cfg.public_base_url:
            kb.button(text="Открыть документацию", url=f"{cfg.public_base_url}/api/docs")
        kb.button(text="⌂ Главное меню", callback_data="menu")
        kb.adjust(1)
        if edit:
            await edit_screen(message, text, reply_markup=kb.as_markup())
        else:
            await message.answer(text, reply_markup=kb.as_markup())

    @router.callback_query(F.data == "api", F.message.chat.type == "private")
    async def api_cb(cq: CallbackQuery) -> None:
        if db.blocked(cq.from_user.id):
            await cq.answer("Доступ закрыт", show_alert=True)
            return
        await cq.answer()
        await show_api(cq.message, cq.from_user.id)

    @router.callback_query(F.data == "apipacks")
    async def api_packages(cq: CallbackQuery) -> None:
        await cq.answer()
        kb = InlineKeyboardBuilder()
        for plan, units in API_TOPUPS.items():
            kb.button(
                text=f"+{units} загрузок · {cfg.price_label(plan)}",
                callback_data=f"pay:{plan}",
            )
        kb.button(text="◀︎ Назад", callback_data="api")
        kb.adjust(1)
        await edit_screen(
            cq.message,
            "<b>Пополнить загрузки</b>\n\n"
            "Выберите пакет ниже. Неиспользованные загрузки сохраняются.\n"
            "<i>Срок доступа к API остаётся прежним.</i>",
            reply_markup=kb.as_markup(),
        )

    @router.message(Command("api"), F.chat.type == "private")
    async def api_cmd(message: Message) -> None:
        if db.blocked(message.from_user.id):
            return
        db.user(
            message.from_user.id,
            message.from_user.username or "",
            message.from_user.full_name,
        )
        await show_api(message, message.from_user.id, edit=False)

    @router.callback_query(
        F.data.in_({"apikey", "apikeyread"}), F.message.chat.type == "private"
    )
    async def api_key(cq: CallbackQuery) -> None:
        user = db.one("select api_until from users where tg_id=?", (cq.from_user.id,))
        if not user or user["api_until"] <= ota.now() or db.blocked(cq.from_user.id):
            await cq.answer("Сначала подключите API", show_alert=True)
            return
        scopes = (
            "read"
            if cq.data == "apikeyread"
            else "read downloads billing accounts keys"
        )
        key = issue_api_key(db, cq.from_user.id, scopes=scopes, days=cfg.api_key_days)
        await cq.answer("Новый ключ создан")
        await cq.message.answer(
            f"<b>Новый основной ключ</b>\n<pre>{key}</pre>\n<blockquote>Сохраните ключ. Предыдущий основной ключ больше не работает.</blockquote>",
        )

    @router.callback_query(F.data == "apirevoke", F.message.chat.type == "private")
    async def api_revoke(cq: CallbackQuery) -> None:
        db._run("update api_keys set state='revoked' where tg_id=?", (cq.from_user.id,))
        await cq.answer("API-ключ отключён")
        await show_api(cq.message, cq.from_user.id)

    # --- мой Apple ID ------------------------------------------------------
    async def show_my_account(message: Message, tg_id: int, edit: bool = True) -> None:
        account = db.account_of(tg_id)
        text, kb = ui.my_account(account), ui.my_account_kb(account)
        if edit:
            await edit_screen(message, text, reply_markup=kb)
        else:
            await message.answer(text, reply_markup=kb)

    @router.callback_query(F.data == "me")
    async def my_account_cb(cq: CallbackQuery, state: FSMContext) -> None:
        await state.clear()
        await cq.answer()
        await show_my_account(cq.message, cq.from_user.id)

    @router.message(Command("apple"))
    async def my_account_cmd(message: Message) -> None:
        await show_my_account(message, message.from_user.id, edit=False)

    @router.callback_query(F.data.startswith("act:"))
    async def account_test(cq: CallbackQuery) -> None:
        account = db.account(int(cq.data.split(":")[1]))
        if not owns(cq.from_user.id, account):
            await cq.answer("Это не ваш аккаунт", show_alert=True)
            return
        await cq.answer("Проверяю…")
        progress = Progress.of(cq.message, account["email"])
        await progress.start("Проверяю Apple ID")
        try:
            await tool.revalidate(account)
        except IpatoolError as exc:
            db.account_status(
                int(account["id"]),
                "need2fa" if exc.needs_2fa else "error",
                str(exc)[:180],
            )
        else:
            db.account_status(int(account["id"]), "ready")
        finally:
            progress.stop()
        fresh = db.account(int(account["id"]))
        if admin_only(cq.from_user.id) and int(fresh["tg_id"]) != cq.from_user.id:
            await edit_screen(
                cq.message, ui.account_card(fresh), reply_markup=ui.account_kb(fresh)
            )
        else:
            await edit_screen(
                cq.message, ui.my_account(fresh), reply_markup=ui.my_account_kb(fresh)
            )

    @router.callback_query(F.data.startswith("acd:"))
    async def account_delete(cq: CallbackQuery) -> None:
        account = db.account(int(cq.data.split(":")[1]))
        if not owns(cq.from_user.id, account):
            await cq.answer("Это не ваш аккаунт", show_alert=True)
            return
        await cq.answer()
        kb = InlineKeyboardBuilder()
        kb.button(text="🚪 Да, отключить", callback_data=f"acdd:{account['id']}")
        kb.button(
            text="◀︎ Отмена",
            callback_data="me" if int(account["tg_id"]) == cq.from_user.id else "acc",
        )
        kb.adjust(1)
        await edit_screen(
            cq.message,
            f"🚪 <b>Отключить Apple ID?</b>\n{ui.LINE}\n"
            f"<code>{ui.esc(account['email'])}</code>\n{ui.THIN}\n"
            f"Сохранённые данные для входа будут удалены.\n"
            f"<i>Уже скачанные файлы останутся у вас.</i>",
            reply_markup=kb.as_markup(),
        )

    @router.callback_query(F.data.startswith("acdd:"))
    async def account_delete_confirm(cq: CallbackQuery) -> None:
        account = db.account(int(cq.data.split(":")[1]))
        if not owns(cq.from_user.id, account):
            await cq.answer("Это не ваш аккаунт", show_alert=True)
            return
        await cq.answer("Отключил")
        mine = int(account["tg_id"]) == cq.from_user.id
        await deps.apple.stop_worker(int(account["id"]))
        db.account_delete(int(account["id"]))
        shutil.rmtree(cfg.accounts_dir / str(account["id"]), ignore_errors=True)
        if mine:
            await show_my_account(cq.message, cq.from_user.id)
        else:
            rows = db.accounts()
            await edit_screen(
                cq.message, ui.accounts_text(rows), reply_markup=ui.accounts_kb(rows)
            )

    # --- мастер входа -----------------------------------------------------
    async def wizard_edit(
        state: FSMContext, bot: Bot, text: str, markup: Any = None
    ) -> None:
        data = await state.get_data()
        if "note" not in data:
            return
        try:
            if data.get("note_photo"):
                await bot.edit_message_caption(
                    caption=text,
                    chat_id=data["chat"],
                    message_id=data["note"],
                    reply_markup=markup,
                )
            else:
                await bot.edit_message_text(
                    text,
                    chat_id=data["chat"],
                    message_id=data["note"],
                    reply_markup=markup,
                )
        except Exception:  # noqa: BLE001 - сообщение могли удалить руками
            pass

    @router.callback_query(F.data == "acadd")
    async def account_add_start(cq: CallbackQuery, state: FSMContext) -> None:
        await cq.answer()
        await edit_screen(
            cq.message,
            ui.wizard(1, 3, "Подключение Apple ID", ui.WIZARD_EMAIL),
            reply_markup=ui.cancel_kb("me"),
        )
        await state.set_state(Auth.email)
        await state.update_data(
            chat=cq.message.chat.id,
            note=cq.message.message_id,
            note_photo=bool(cq.message.photo),
        )

    @router.message(Auth.email)
    async def account_add_email(message: Message, state: FSMContext) -> None:
        email = (message.text or "").strip()
        await hush(message)
        if "@" not in email:
            await wizard_edit(
                state,
                message.bot,
                ui.wizard(
                    1,
                    3,
                    "Подключение Apple ID",
                    "⚠️ Это не похоже на почту.\n\n" + ui.WIZARD_EMAIL,
                ),
                ui.cancel_kb("me"),
            )
            return
        await state.update_data(email=email)
        await state.set_state(Auth.password)
        await wizard_edit(
            state,
            message.bot,
            ui.wizard(2, 3, ui.esc(email), ui.WIZARD_PASSWORD),
            ui.cancel_kb("me"),
        )

    @router.message(Auth.password)
    async def account_add_password(message: Message, state: FSMContext) -> None:
        password = message.text or ""
        tg_id = message.from_user.id
        await hush(message)
        data = await state.get_data()
        account_id = db.account_add(
            tg_id, data["email"], password, secrets.token_urlsafe(24)
        )
        await state.update_data(account=account_id)
        await _try_login(message.bot, state, account_id, None)

    @router.message(Auth.code)
    async def account_add_code(message: Message, state: FSMContext) -> None:
        code = "".join(ch for ch in (message.text or "") if ch.isdigit())
        await hush(message)
        data = await state.get_data()
        if len(code) < 4:
            await wizard_edit(
                state,
                message.bot,
                ui.wizard(
                    3,
                    3,
                    ui.esc(data.get("email", "")),
                    "⚠️ Нужны только цифры кода.\n\n" + ui.WIZARD_CODE,
                ),
                ui.cancel_kb("me"),
            )
            return
        await _try_login(message.bot, state, int(data["account"]), code)

    async def _try_login(
        bot: Bot, state: FSMContext, account_id: int, code: str | None
    ) -> None:
        data = await state.get_data()
        email = data.get("email", "")
        progress = Progress.at(
            bot,
            data["chat"],
            data["note"],
            email,
            3,
            photo=bool(data.get("note_photo")),
        )
        progress.step = 2
        await progress.start("Проверяю код" if code else "Вхожу в Apple ID")
        account = db.account(account_id)
        try:
            await tool.login(account, code)
        except IpatoolError as exc:
            progress.stop()
            if exc.needs_2fa:
                db.account_status(account_id, "need2fa", "ждёт код проверки")
                await state.set_state(Auth.code)
                await wizard_edit(
                    state,
                    bot,
                    ui.wizard(3, 3, ui.esc(email), ui.WIZARD_CODE),
                    ui.cancel_kb("me"),
                )
                return
            db.account_status(account_id, "error", str(exc)[:180])
            kb = InlineKeyboardBuilder()
            kb.button(text="🔁 Попробовать снова", callback_data="acadd")
            kb.button(text="◀︎ Назад", callback_data="me")
            kb.adjust(1)
            await wizard_edit(
                state,
                bot,
                ui.failed(str(exc), "Проверьте почту и пароль."),
                kb.as_markup(),
            )
            await state.clear()
            return
        finally:
            progress.stop()
        db.account_status(account_id, "ready")
        db.link_identity(email, int(account["tg_id"]))
        fresh = db.account(account_id)
        await wizard_edit(
            state,
            bot,
            f"✅ <b>Apple ID подключён</b>\n{ui.LINE}\n" + ui.my_account(fresh),
            ui.my_account_kb(fresh),
        )
        await state.clear()

    # --- админ-панель ------------------------------------------------------
    async def show_admin(message: Message, edit: bool = True) -> None:
        from .production import maintenance_enabled

        data = db.stats()
        data["api_clients"] = db.one(
            "select count(*) from users where api_until>? and api_enabled=1 and blocked=0",
            (ota.now(),),
        )[0]
        data["api_keys"] = db.one(
            "select count(*) from api_keys where state='active' and expires_at>?",
            (ota.now(),),
        )[0]
        data["api_jobs"] = db.one(
            "select count(*) from api_jobs where status in ('queued','downloading')"
        )[0]
        files = db.files_stats()
        text = ui.admin_panel(
            data,
            files,
            data["revenue"] / 100,
            "₽",
        )
        if edit:
            await edit_screen(
                message,
                text,
                reply_markup=ui.admin_kb(
                    maintenance_enabled(db, "bot"), maintenance_enabled(db, "api")
                ),
            )
        else:
            await message.answer(
                text,
                reply_markup=ui.admin_kb(
                    maintenance_enabled(db, "bot"), maintenance_enabled(db, "api")
                ),
            )

    @router.callback_query(F.data.startswith("maintenance:"))
    async def admin_maintenance(cq: CallbackQuery) -> None:
        if not admin_only(cq.from_user.id):
            return
        from .production import maintenance_enabled, set_maintenance

        service = cq.data.split(":", 1)[1]
        if service not in ("bot", "api"):
            return
        enabled = not maintenance_enabled(db, service)
        set_maintenance(db, service, enabled)
        db._run(
            "insert into api_audit(actor,tg_id,action,target,created_at) values (?,?,?,?,?)",
            (
                cq.from_user.id,
                cq.from_user.id,
                "maintenance",
                service + ":" + str(int(enabled)),
                now(),
            ),
        )
        await cq.answer("Техработы включены" if enabled else "Работа возобновлена")
        await show_admin(cq.message)

    @router.callback_query(
        F.data.in_({"admin:health", "admin:payments", "admin:fraud", "admin:backup"})
    )
    async def admin_operations(cq: CallbackQuery):
        if not admin_only(cq.from_user.id):
            return
        await cq.answer()
        section = cq.data.split(":")[1] if ":" in cq.data else "health"
        if section == "backup":
            from .production import backup_database

            path = await waiting(
                cq.message,
                asyncio.to_thread(backup_database, cfg),
                "Сохраняю резервную копию",
            )
            text = f"<b>Резервная копия сохранена</b>\nФайл зашифрован.\n<code>{ui.esc(path.name)}</code>"
        elif section == "fraud":
            rows = db.all(
                "select email,count(distinct tg_id) count from identities group by email having count(distinct tg_id)>1 order by count desc limit 20"
            )
            text = (
                "<b>Общие Apple ID</b>\nБесплатный лимит общий для связанных Telegram-аккаунтов. Платные загрузки учитываются отдельно.\n\n"
                + (
                    "\n".join(
                        f"{ui.esc(row['email'])} · {row['count']} аккаунта"
                        for row in rows
                    )
                    or "Совпадений нет."
                )
            )
        elif section == "payments":
            rows = db.all(
                "select tg_id,plan,amount,status,created_at from orders order by created_at desc limit 12"
            )
            text = "<b>Последние оплаты</b>\n\n" + (
                "\n\n".join(
                    f"<code>{row['tg_id']}</code> · {ui.esc(ui.PLAN_TITLES.get(row['plan'], row['plan']))} · {row['amount'] / 100:g} ₽\n{ {'complete': 'Оплачено', 'pending': 'Ожидает оплаты', 'canceled': 'Отменено'}.get(row['status'], 'Не завершено') } · {ui.when(row['created_at'], with_time=True)}"
                    for row in rows
                )
                or "Оплат пока нет."
            )
        else:
            import json
            import time
            from .production import maintenance_enabled

            lines = [
                "<b>Состояние сервисов</b>",
                f"Свободно на диске: {ota.human_size(shutil.disk_usage(cfg.data_dir).free)}",
            ]
            for service in ("bot", "api"):
                try:
                    health = json.loads(
                        (cfg.data_dir / "logs" / f"{service}.health.json").read_text()
                    )
                    online = time.time() - health["updated_at"] < 120
                except (OSError, ValueError, KeyError):
                    online = False
                lines.append(
                    f"{service.upper()}: {'На связи' if online else 'Нет ответа'} · {'Техработы' if maintenance_enabled(db, service) else 'Открыт'}"
                )
            lines.append(
                f"Загрузок в работе: {db.one('select count(*) from download_claims where expires_at>?', (now(),))[0]}"
            )
            count = db.one(
                "select count(*) from api_jobs where status in ('queued','downloading')"
            )[0]
            lines.append(f"API в очереди: {count}")
            lines.append(
                f"Резервных копий: {len(list((cfg.data_dir / 'backups').glob('*.enc')))}"
            )
            text = "\n".join(lines)
        await edit_screen(cq.message, text, ui.back_kb("adm", "В панель"))

    @router.callback_query(F.data == "adm")
    async def admin_cb(cq: CallbackQuery, state: FSMContext) -> None:
        if not admin_only(cq.from_user.id):
            await cq.answer("Только для админов", show_alert=True)
            return
        await state.clear()
        await cq.answer()
        await show_admin(cq.message)

    @router.message(Command("admin"))
    async def admin_cmd(message: Message) -> None:
        if not admin_only(message.from_user.id):
            return
        await show_admin(message, edit=False)

    @router.callback_query(F.data == "acc")
    async def accounts_cb(cq: CallbackQuery) -> None:
        if not admin_only(cq.from_user.id):
            await cq.answer("Только для админов", show_alert=True)
            return
        await cq.answer()
        rows = db.accounts()
        await edit_screen(
            cq.message, ui.accounts_text(rows), reply_markup=ui.accounts_kb(rows)
        )

    @router.callback_query(F.data.startswith("ac:"))
    async def account_card_cb(cq: CallbackQuery) -> None:
        if not admin_only(cq.from_user.id):
            return
        await cq.answer()
        account = db.account(int(cq.data.split(":")[1]))
        if account is None:
            rows = db.accounts()
            await edit_screen(
                cq.message, ui.accounts_text(rows), reply_markup=ui.accounts_kb(rows)
            )
            return
        await edit_screen(
            cq.message, ui.account_card(account), reply_markup=ui.account_kb(account)
        )

    @router.callback_query(F.data.startswith("admu:"))
    async def admin_users(cq: CallbackQuery) -> None:
        if not admin_only(cq.from_user.id):
            return
        await cq.answer()
        page = max(0, int(cq.data.split(":")[1]))
        total = db.users_count()
        rows = db.users_page(USERS_PAGE, page * USERS_PAGE)
        kb = InlineKeyboardBuilder()
        for item in rows:
            who = (
                f"@{item['username']}"
                if item["username"]
                else (item["name"] or item["tg_id"])
            )
            kb.button(
                text=f"{'🚫 ' if item['blocked'] else ''}{who}",
                callback_data=f"admui:{item['tg_id']}",
            )
        kb.adjust(1)
        nav = InlineKeyboardBuilder()
        nav.button(text="◀︎", callback_data=f"admu:{page - 1}" if page else "noop")
        nav.button(text=f"{page + 1}", callback_data="noop")
        nav.button(
            text="▶︎",
            callback_data=f"admu:{page + 1}"
            if (page + 1) * USERS_PAGE < total
            else "noop",
        )
        nav.adjust(3)
        tail = InlineKeyboardBuilder()
        tail.button(text="◀︎ В панель", callback_data="adm")
        kb.attach(nav)
        kb.attach(tail)
        await edit_screen(
            cq.message,
            ui.users_text(rows, total, page, USERS_PAGE),
            reply_markup=kb.as_markup(),
        )

    async def show_user(message: Message, tg_id: int) -> None:
        item = db.one("select * from users where tg_id=?", (tg_id,))
        if item is None:
            await edit_screen(
                message, "Нет такого пользователя.", reply_markup=ui.back_kb("admu:0")
            )
            return
        uses = db.one(
            "select count(*) c from history where tg_id=? and status='ok'", (tg_id,)
        )
        await edit_screen(
            message,
            ui.user_card(item, int(uses["c"]) if uses else 0, db.sub_active(tg_id)),
            reply_markup=ui.user_kb(item),
        )

    @router.callback_query(F.data.startswith("admui:"))
    async def admin_user(cq: CallbackQuery) -> None:
        if not admin_only(cq.from_user.id):
            return
        await cq.answer()
        await show_user(cq.message, int(cq.data.split(":")[1]))

    @router.callback_query(F.data.startswith("admus:"))
    async def admin_user_sub(cq: CallbackQuery) -> None:
        if not admin_only(cq.from_user.id):
            return
        tg_id = int(cq.data.split(":")[1])
        until = db.extend_sub(tg_id, 30)
        await cq.answer(f"Подписка до {ui.when(until)}")
        await show_user(cq.message, tg_id)

    @router.callback_query(F.data.startswith("admuc:"))
    async def admin_user_credit(cq: CallbackQuery) -> None:
        if not admin_only(cq.from_user.id):
            return
        tg_id = int(cq.data.split(":")[1])
        db.add_credits(tg_id, 1)
        await cq.answer("Добавил установку")
        await show_user(cq.message, tg_id)

    @router.callback_query(F.data.startswith("admub:"))
    async def admin_user_block(cq: CallbackQuery) -> None:
        if not admin_only(cq.from_user.id):
            return
        tg_id = int(cq.data.split(":")[1])
        blocked = db.blocked(tg_id)
        db.set_blocked(tg_id, not blocked)
        await cq.answer("Разблокировал" if blocked else "Заблокировал")
        await show_user(cq.message, tg_id)

    @router.callback_query(F.data == "admf")
    async def admin_files(cq: CallbackQuery) -> None:
        if not admin_only(cq.from_user.id):
            return
        await cq.answer()
        count, size = db.files_stats()
        await edit_screen(
            cq.message,
            ui.files_text(
                count, size, cfg.link_ttl_free_hours, cfg.link_ttl_paid_hours
            ),
            reply_markup=ui.files_kb(),
        )

    @router.callback_query(F.data.in_({"admfc", "admfa"}))
    async def admin_files_clean(cq: CallbackQuery) -> None:
        if not admin_only(cq.from_user.id):
            return
        await cq.answer("Очищаю файлы…")
        if cfg.bridge_enabled:
            from .bridge import revoke_file

            rows = db.files_all() if cq.data == "admfa" else db.files_expired()
            for row in rows:
                if row["remote"]:
                    await revoke_file(cfg, row["token"])
        count, size = ota.purge(cfg, db, everything=cq.data == "admfa")
        await cq.message.answer(f"Удалено файлов: {count} ({ota.human_size(size)}).")
        left_count, left_size = db.files_stats()
        await edit_screen(
            cq.message,
            ui.files_text(
                left_count, left_size, cfg.link_ttl_free_hours, cfg.link_ttl_paid_hours
            ),
            reply_markup=ui.files_kb(),
        )

    @router.callback_query(F.data == "admb")
    async def admin_broadcast(cq: CallbackQuery, state: FSMContext) -> None:
        if not admin_only(cq.from_user.id):
            return
        await cq.answer()
        await state.set_state(Adm.broadcast)
        await state.update_data(chat=cq.message.chat.id, note=cq.message.message_id)
        await edit_screen(
            cq.message, ui.BROADCAST_ASK, reply_markup=ui.cancel_kb("adm")
        )

    @router.message(Adm.broadcast)
    async def admin_broadcast_preview(message: Message, state: FSMContext) -> None:
        if not admin_only(message.from_user.id):
            return
        text = message.html_text or ""
        await hush(message)
        await state.clear()
        deps.broadcast[message.from_user.id] = text
        kb = InlineKeyboardBuilder()
        kb.button(text=f"📣 Отправить ({len(db.user_ids())})", callback_data="admbs")
        kb.button(text="◀︎ Отмена", callback_data="adm")
        kb.adjust(1)
        await message.answer(
            f"📣 <b>Так это увидят люди</b>\n{ui.LINE}\n\n{text}\n\n{ui.THIN}\n<i>Отправляем?</i>",
            reply_markup=kb.as_markup(),
        )

    @router.callback_query(F.data == "admbs")
    async def admin_broadcast_send(cq: CallbackQuery) -> None:
        if not admin_only(cq.from_user.id):
            return
        text = deps.broadcast.get(cq.from_user.id)
        if not text:
            await cq.answer("Текст потерялся, наберите заново", show_alert=True)
            return
        await cq.answer("Отправляю")
        targets = db.user_ids()
        sent = failed = 0
        for index, tg_id in enumerate(targets, 1):
            try:
                await cq.bot.send_message(tg_id, text)
                sent += 1
            except Exception:  # noqa: BLE001 - заблокировавшие бота это норма
                failed += 1
            # Telegram пропускает около 30 сообщений в секунду — держимся ниже.
            await asyncio.sleep(0.05)
            if index % 25 == 0:
                try:
                    await edit_screen(
                        cq.message,
                        f"📣 <b>Рассылка</b>\n{ui.LINE}\n{index} из {len(targets)}…",
                    )
                except Exception:  # noqa: BLE001
                    pass
        await edit_screen(
            cq.message,
            f"📣 <b>Рассылка закончена</b>\n{ui.LINE}\n"
            f"✅ Доставлено: <b>{sent}</b>\n⚠️ Не дошло: <b>{failed}</b>",
            reply_markup=ui.back_kb("adm", "◀︎ В панель"),
        )

    @router.message(Command("grant"))
    async def grant(message: Message) -> None:
        if not admin_only(message.from_user.id):
            return
        parts = (message.text or "").split()
        if len(parts) < 3 or not parts[1].isdigit():
            await message.answer(
                "Формат: <code>/grant tg_id 30</code> или <code>/grant tg_id +5</code>"
            )
            return
        tg_id = int(parts[1])
        db.user(tg_id)
        if parts[2].startswith("+"):
            db.add_credits(tg_id, int(parts[2][1:]))
            await message.answer(
                f"🎟 Выдал {parts[2][1:]} установок пользователю <code>{tg_id}</code>."
            )
            return
        until = db.extend_sub(tg_id, int(parts[2]))
        await message.answer(
            f"💎 Подписка для <code>{tg_id}</code> до {ui.when(until)}."
        )

    # --- ссылка или название прямо в чат ----------------------------------
    @router.message(StateFilter(None), F.text, ~F.text.startswith("/"))
    async def free_text(message: Message, state: FSMContext) -> None:
        if await state.get_state():
            return
        if db.blocked(message.from_user.id):
            return
        adam = _adam_from_text(message.text or "")
        if adam:
            await _send_card(message, adam, message.from_user.id)
            return
        await search_run(message, state)

    from .apple_screens import build_apple_screens

    router.include_router(build_apple_screens(deps, edit_screen))
    router.include_router(build_management(deps, edit_screen))
    return router


def _adam_from_text(text: str) -> str | None:
    """Достаёт adamId из ссылки App Store или из голых цифр."""
    value = text.strip()
    if value.isdigit() and 6 <= len(value) <= 12:
        return value
    marker = "/id"
    if marker in value:
        tail = value.split(marker)[-1]
        digits = ""
        for char in tail:
            if char.isdigit():
                digits += char
            else:
                break
        if digits:
            return digits
    return None
