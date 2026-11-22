from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from signum_bot.config import Config
from signum_bot.session_vault import SessionVault
from signum_bot.telegram_sessions import TelegramSessions


def main():
    cfg = Config.load()
    vault = SessionVault(cfg.data_dir)
    count = 0
    for home in cfg.accounts_dir.iterdir():
        if not home.is_dir() or not home.name.lstrip("-").isdigit():
            continue
        if (home / ".ipatool").is_dir() or vault.archive(home).exists():
            with vault.open(home):
                pass
            count += 1
    telegram = TelegramSessions(cfg.data_dir)
    paths = [cfg.data_dir / "sender.session"]
    if cfg.premium_session:
        premium = Path(cfg.premium_session)
        paths.append(premium if premium.is_absolute() else cfg.data_dir / premium)
    tg_count = 0
    try:
        for path in dict.fromkeys(paths):
            if telegram.exists(path):
                telegram.acquire(path)
                tg_count += 1
    finally:
        telegram.close()
    print(f"Encrypted Apple sessions: {count}; Telegram sessions: {tg_count}")


if __name__ == "__main__":
    main()
