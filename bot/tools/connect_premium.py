from __future__ import annotations

import argparse
import asyncio
import getpass
import os
import re
import sys
from pathlib import Path

import qrcode
from aiogram import Bot
from telethon import TelegramClient, functions, types, utils
from telethon.errors import SessionPasswordNeededError

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))
if sys.platform == "win32":
    os.environ.setdefault("DATA_DIR", str(ROOT / "data"))

from signum_bot.config import Config  # noqa: E402


def save_settings(settings: dict[str, str]) -> None:
    path = ROOT / ".env"
    content = path.read_text(encoding="utf-8") if path.exists() else ""
    for key, value in settings.items():
        line = f"{key}={value}"
        pattern = rf"^{re.escape(key)}=.*$"
        if re.search(pattern, content, flags=re.M):
            content = re.sub(pattern, lambda _: line, content, flags=re.M)
        else:
            content = content.rstrip() + "\n" + line + "\n"
    temporary = path.with_name(".env.premium.tmp")
    temporary.write_text(content, encoding="utf-8")
    temporary.replace(path)


async def qr_sign_in(client: TelegramClient, output: Path) -> None:
    for _ in range(6):
        login = await client.qr_login()
        code = qrcode.QRCode(border=2)
        code.add_data(login.url)
        code.make(fit=True)
        code.make_image().save(output)
        print("Telegram на телефоне → Настройки → Устройства → Подключить устройство.")
        print(f"Отсканируйте QR-код: {output}")
        if sys.platform == "win32":
            os.startfile(output)
        else:
            code.print_ascii(invert=True)
        try:
            await login.wait(timeout=30)
            return
        except asyncio.TimeoutError:
            print("Обновляю QR-код…")
        except SessionPasswordNeededError:
            await client.sign_in(password=getpass.getpass("Облачный пароль Telegram: "))
            return
    raise RuntimeError("Время входа истекло. Запустите команду ещё раз.")


async def connect(args) -> None:
    cfg = Config.load()
    cfg.prepare()
    if not cfg.api_id or not cfg.api_hash:
        raise RuntimeError("Заполните TG_API_ID и TG_API_HASH в .env")
    session = (
        Path(cfg.premium_session)
        if cfg.premium_session
        else cfg.data_dir / "premium-upload.session"
    )
    if not session.is_absolute():
        session = cfg.data_dir / session
    session = session.with_suffix(".session")
    session.parent.mkdir(parents=True, exist_ok=True)
    qr_path = cfg.data_dir / "premium-login.png"
    client = TelegramClient(
        str(session),
        cfg.api_id,
        cfg.api_hash,
        device_model="Signum Upload",
        app_version="1.0",
    )
    await client.connect()
    try:
        if not await client.is_user_authorized():
            if args.phone:
                await client.start(
                    phone=lambda: input("Номер телефона с кодом страны: ").strip(),
                    code_callback=lambda: getpass.getpass("Код входа из Telegram: "),
                    password=lambda: getpass.getpass("Облачный пароль Telegram: "),
                )
            else:
                await qr_sign_in(client, qr_path)
        me = await client.get_me()
        if me.bot or not me.premium:
            raise RuntimeError("На подключённом аккаунте нет Telegram Premium.")
        relay = args.relay or cfg.upload_relay
        async with Bot(cfg.token) as bot:
            bot_me = await bot.get_me()
            bot_user = await client.get_input_entity(bot_me.username)
            if not relay:
                if not args.create_relay:
                    raise RuntimeError(
                        "Укажите --create-relay, чтобы создать закрытый служебный канал."
                    )
                result = await client(
                    functions.channels.CreateChannelRequest(
                        title="Signum · передача файлов",
                        about="Временная передача больших приложений в Signum Bot.",
                        broadcast=True,
                    )
                )
                channel = result.chats[0]
                relay = utils.get_peer_id(channel)
                # Сохраняем сразу: повторный запуск использует уже созданный канал.
                save_settings({"TG_UPLOAD_RELAY": str(relay)})
            else:
                await client.get_dialogs()
                channel = await client.get_entity(relay)
            if (
                not isinstance(channel, types.Channel)
                or not channel.broadcast
                or channel.username
            ):
                raise RuntimeError(
                    "Нужен закрытый служебный канал без публичного адреса."
                )
            await client(
                functions.channels.EditAdminRequest(
                    channel=channel,
                    user_id=bot_user,
                    admin_rights=types.ChatAdminRights(
                        post_messages=True, delete_messages=True
                    ),
                    rank="Signum",
                )
            )
            member = await bot.get_chat_member(relay, bot_me.id)
            if member.status not in {"administrator", "creator"}:
                raise RuntimeError("Не удалось добавить бота администратором канала.")
        save_settings(
            {
                "TG_PREMIUM_SESSION": str(session.resolve()),
                "TG_UPLOAD_RELAY": str(relay),
                "MTPROTO_MAX_MB": "4000",
            }
        )
        print("Premium-отправитель подключён. Перезапустите бота.")
        print("Бесплатные загрузки остаются до 2 ГБ; платные — до лимита Premium.")
    finally:
        await client.disconnect()
        qr_path.unlink(missing_ok=True)
        if session.exists():
            os.chmod(session, 0o600)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Подключить Telegram Premium для передачи больших файлов"
    )
    parser.add_argument(
        "--create-relay",
        action="store_true",
        help="Создать закрытый служебный канал и добавить туда бота",
    )
    parser.add_argument(
        "--relay", type=int, help="ID существующего закрытого служебного канала"
    )
    parser.add_argument(
        "--phone", action="store_true", help="Войти по номеру и коду вместо QR"
    )
    args = parser.parse_args()
    try:
        asyncio.run(connect(args))
    except KeyboardInterrupt:
        print("Вход отменён.")
    except Exception as exc:
        # Ошибки RPC не печатают ключи или содержимое сессии.
        print(f"Подключение не завершено: {exc}")
        raise SystemExit(1) from None


if __name__ == "__main__":
    main()
