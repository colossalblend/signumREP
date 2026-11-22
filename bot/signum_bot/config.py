from __future__ import annotations

import os
import platform
import sys
from dataclasses import dataclass, field
from pathlib import Path

BASE_DIR = Path(__file__).resolve().parent.parent


def default_ipatool() -> str:
    root = BASE_DIR.parent
    if sys.platform == "win32":
        candidates = [
            root / "apple-tools/windows-x64/v6/ipatool.exe",
            root / "apple-tools/windows-x64/v5/ipatool.exe",
        ]
    elif sys.platform == "darwin":
        arch = "macos-arm64" if platform.machine() == "arm64" else "macos-x64"
        candidates = [
            root / f"apple-tools/{arch}/v6/ipatool",
            root / f"apple-tools/{arch}/v5/ipatool",
        ]
    else:
        arch = (
            "linux_arm64" if platform.machine() in {"aarch64", "arm64"} else "linux_x64"
        )
        candidates = [root / arch / "ipatool-v6", root / arch / "ipatool-v5"]
    return next(
        (str(path.resolve()) for path in candidates if path.is_file()), "ipatool"
    )


def load_env(path: Path | None = None) -> None:
    """Читает .env рядом с ботом. Реальное окружение всегда важнее файла."""
    path = path or BASE_DIR / ".env"
    if not path.exists():
        return
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        os.environ.setdefault(key.strip(), value.strip().strip('"').strip("'"))


def _int(name: str, default: int) -> int:
    try:
        return int(os.getenv(name, "").strip() or default)
    except ValueError:
        return default


@dataclass(frozen=True)
class Config:
    token: str
    admins: set[int]
    data_dir: Path
    ipatool: str
    concurrency: int
    api_base_url: str
    public_base_url: str
    http_host: str
    http_port: int
    tls_port: int
    tls_hosts: list[str]
    link_ttl_free_hours: int
    link_ttl_paid_hours: int
    free_per_week: int
    prices: dict[str, int]
    provider_token: str
    local_api_url: str
    api_id: int
    api_hash: str
    upload_limit_mb: int
    max_mb_free: int
    max_mb_sub: int
    mtproto_max_mb: int
    zsign: dict[str, str] = field(default_factory=dict)
    ipatool_engine: str = "v6"
    ipatool_metadata: str = ""
    session_encryption: bool = False
    premium_session: str = ""
    upload_relay: int = 0
    payment_mode: str = "demo"
    api_downloads: int = 1000
    api_rate_limit: int = 30
    api_key_days: int = 30
    yookassa_shop_id: str = ""
    yookassa_secret_key: str = field(default="", repr=False)
    yookassa_return_url: str = "https://t.me/signummbot"
    api_auth_ttl: int = 600
    api_accounts_max: int = 10
    api_global_requests: int = 32
    api_global_rate: int = 1200
    api_embedded: bool = True
    production: bool = False
    bridge_enabled: bool = False
    api_trusted_proxies: tuple[str, ...] = ()

    @classmethod
    def load(cls, *, require_bot_token: bool = True) -> "Config":
        load_env()
        token = os.getenv("BOT_TOKEN", "").strip()
        if not token and require_bot_token:
            raise SystemExit("BOT_TOKEN не задан: заполните bot/.env")
        admins = {
            int(part)
            for part in os.getenv("BOT_ADMINS", "").replace(";", ",").split(",")
            if part.strip().isdigit()
        }
        data_dir = Path(os.getenv("DATA_DIR", "").strip() or BASE_DIR / "data")
        return cls(
            token=token,
            admins=admins,
            data_dir=data_dir,
            ipatool=os.getenv("IPATOOL", "").strip() or default_ipatool(),
            ipatool_engine=os.getenv("IPATOOL_ENGINE", "v6").strip().lower(),
            ipatool_metadata=os.getenv("IPATOOL_METADATA", "").strip(),
            session_encryption=os.getenv("SESSION_ENCRYPTION", "0") == "1",
            concurrency=max(1, _int("CONCURRENCY", 2)),
            api_base_url=(
                os.getenv("API_BASE_URL", "").strip() or "https://api.appbank.pw/signum"
            ).rstrip("/"),
            public_base_url=os.getenv("PUBLIC_BASE_URL", "").strip().rstrip("/"),
            http_host=os.getenv("HTTP_HOST", "").strip() or "127.0.0.1",
            http_port=_int("HTTP_PORT", 8088),
            # Локальный HTTPS с самоподписанным сертификатом: iOS ставит
            # приложения только с доверенного адреса, а в своей сети другого
            # взять неоткуда.
            tls_port=_int("TLS_PORT", 0),
            tls_hosts=[
                part.strip()
                for part in os.getenv("TLS_HOSTS", "").replace(";", ",").split(",")
                if part.strip()
            ],
            link_ttl_free_hours=max(1, _int("LINK_TTL_FREE_HOURS", 1)),
            link_ttl_paid_hours=max(1, _int("LINK_TTL_PAID_HOURS", 12)),
            free_per_week=max(0, _int("FREE_PER_WEEK", 3)),
            prices={
                "single": _int("PRICE_SINGLE", 29),
                "week": _int("PRICE_WEEK", 79),
                "month": _int("PRICE_MONTH", 199),
                "halfyear": _int("PRICE_HALFYEAR", 899),
                "year": _int("PRICE_YEAR", 1490),
                "api": _int("PRICE_API", 299),
                "api100": _int("PRICE_API_100", 49),
                "api500": _int("PRICE_API_500", 149),
                "api1000": _int("PRICE_API_1000", 249),
            },
            payment_mode=os.getenv("PAYMENT_MODE", "demo").strip().lower(),
            api_downloads=max(1, _int("API_DOWNLOADS", 1000)),
            api_rate_limit=max(1, _int("API_RATE_LIMIT", 30)),
            api_key_days=max(1, _int("API_KEY_DAYS", 30)),
            api_auth_ttl=max(60, min(1800, _int("API_AUTH_TTL", 600))),
            api_accounts_max=max(1, min(50, _int("API_ACCOUNTS_MAX", 10))),
            api_global_requests=max(4, _int("API_GLOBAL_REQUESTS", 32)),
            api_global_rate=max(60, _int("API_GLOBAL_RATE", 1200)),
            api_embedded=os.getenv("API_EMBEDDED", "1").strip() != "0",
            production=os.getenv("APP_ENV", "development").strip().lower()
            == "production",
            bridge_enabled=os.getenv("BRIDGE_ENABLED", "0").strip() == "1",
            api_trusted_proxies=tuple(
                part.strip()
                for part in os.getenv("API_TRUSTED_PROXIES", "").split(",")
                if part.strip()
            ),
            yookassa_shop_id=os.getenv("YOOKASSA_SHOP_ID", "").strip(),
            yookassa_secret_key=os.getenv("YOOKASSA_SECRET_KEY", "").strip(),
            yookassa_return_url=os.getenv(
                "YOOKASSA_RETURN_URL", "https://t.me/signummbot"
            ).strip(),
            provider_token=os.getenv("PAYMENT_PROVIDER_TOKEN", "").strip(),
            local_api_url=os.getenv("LOCAL_BOT_API_URL", "").strip().rstrip("/"),
            # Ключи с my.telegram.org: без них файлы крупнее 50 МБ уйдут ссылкой.
            api_id=_int("TG_API_ID", 0),
            api_hash=os.getenv("TG_API_HASH", "").strip(),
            # 50 МБ — потолок обычного Bot API. Свой telegram-bot-api поднимает
            # его до 2000 МБ, всё что больше уходит прямой ссылкой.
            upload_limit_mb=_int(
                "TG_UPLOAD_MB",
                2000 if os.getenv("LOCAL_BOT_API_URL", "").strip() else 50,
            ),
            max_mb_free=_int("MAX_MB_FREE", 2048),
            max_mb_sub=_int("MAX_MB_SUB", 4096),
            # Telegram пропускает 2 ГБ на файл, у Premium-аккаунта — 4 ГБ.
            mtproto_max_mb=_int("MTPROTO_MAX_MB", 2000),
            premium_session=os.getenv("TG_PREMIUM_SESSION", "").strip(),
            upload_relay=_int("TG_UPLOAD_RELAY", 0),
            zsign={
                "bin": os.getenv("ZSIGN", "").strip(),
                "cert": os.getenv("ZSIGN_CERT", "").strip(),
                "key": os.getenv("ZSIGN_KEY", "").strip(),
                "password": os.getenv("ZSIGN_PASSWORD", "").strip(),
                "prov": os.getenv("ZSIGN_PROV", "").strip(),
            },
        )

    # --- производные пути ---
    @property
    def db_path(self) -> Path:
        return self.data_dir / "signum.db"

    @property
    def files_dir(self) -> Path:
        return self.data_dir / "files"

    @property
    def accounts_dir(self) -> Path:
        return self.data_dir / "accounts"

    def prepare(self) -> None:
        for path in (self.data_dir, self.files_dir, self.accounts_dir):
            path.mkdir(parents=True, exist_ok=True)

    @property
    def currency(self) -> str:
        return "RUB"

    def amount(self, plan: str) -> int:
        """Сумма в копейках."""
        return self.prices[plan] * 100

    def price_label(self, plan: str) -> str:
        return f"{self.prices[plan]} ₽"

    # --- размеры ---
    def link_hours(self, paid_with: str) -> int:
        return (
            self.link_ttl_paid_hours
            if paid_with in {"sub", "credit", "api"}
            else self.link_ttl_free_hours
        )

    @property
    def upload_limit(self) -> int:
        """Сколько байт бот может отправить документом в чат."""
        return self.upload_limit_mb * 1024 * 1024

    def max_bytes(self, subscriber: bool) -> int:
        """Потолок размера приложения по тарифу."""
        return (self.max_mb_sub if subscriber else self.max_mb_free) * 1024 * 1024
