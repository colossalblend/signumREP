"""Быстрая самопроверка: python tests/test_bot.py (из папки bot)."""

from __future__ import annotations

import os
import plistlib
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

os.environ["BOT_TOKEN"] = "test:token"
os.environ["DATA_DIR"] = tempfile.mkdtemp(prefix="signum-bot-")
os.environ["FREE_PER_WEEK"] = "3"
os.environ["TG_API_ID"] = "0"
os.environ["TG_API_HASH"] = ""

from signum_bot import ota  # noqa: E402
from signum_bot.cards import Cards  # noqa: E402
from signum_bot.config import Config  # noqa: E402
from signum_bot.db import Db  # noqa: E402
from signum_bot.handlers import CANCEL_LIMIT, Deps, Job, _adam_from_text  # noqa: E402
from signum_bot.ipatool import Ipatool, IpatoolError, parse_percent  # noqa: E402
from signum_bot.sender import MTPROTO_MAX, FileSender  # noqa: E402
from signum_bot.store import Store, normalize  # noqa: E402


def test_repack() -> None:
    """Пересборка должна чинить архив, но не трогать его содержимое."""
    import plistlib as pl
    import zipfile
    from pathlib import Path

    ipa = Path(os.environ["DATA_DIR"]) / "fake.ipa"
    info = pl.dumps(
        {"CFBundleIdentifier": "com.test.app", "CFBundleShortVersionString": "2.0"}
    )
    with zipfile.ZipFile(ipa, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("META-INF/com.apple.ZipMetadata.plist", b"stale")
        z.writestr("Payload/Test.app/Info.plist", info)
        z.writestr("Payload/Test.app/SC_Info/Test.sinf", b"license")
        z.writestr("iTunesMetadata.plist", pl.dumps({"userName": "a@b.c"}))

    before = ota.ipa_facts(ipa)
    ota.repack(ipa)
    after = ota.ipa_facts(ipa)

    with zipfile.ZipFile(ipa) as z:
        names = z.namelist()
        assert names[0] == "iTunesMetadata.plist", "метаданные должны идти первыми"
        assert not any(n.startswith("META-INF") for n in names), (
            "устаревшая карта Apple убрана"
        )
        assert "Payload/Test.app/SC_Info/Test.sinf" in names, "лицензию терять нельзя"
        assert z.read("Payload/Test.app/Info.plist") == info
    assert (
        before
        == after
        == {"bundle_id": "com.test.app", "version": "2.0", "encrypted": True}
    )
    ipa.unlink()


def test_manifest() -> None:
    data = plistlib.loads(
        ota.manifest(
            "https://x/f/t.ipa", "ru.sber.app", "17.6", "Сбер", "https://x/i.png"
        )
    )
    item = data["items"][0]
    assert item["metadata"]["bundle-identifier"] == "ru.sber.app"
    assert item["metadata"]["bundle-version"] == "17.6"
    assert item["metadata"]["kind"] == "software"
    # ровно один ресурс: лишние картинки iOS тянет во время установки и валит
    # её, если адрес недоступен
    assert item["assets"] == [{"kind": "software-package", "url": "https://x/f/t.ipa"}]


def test_size() -> None:
    assert ota.human_size(0) == "0 Б"
    assert ota.human_size(2048) == "2 КБ"
    assert ota.human_size(150 * 1024 * 1024) == "150.0 МБ"


def test_adam_from_text() -> None:
    assert _adam_from_text("564177498") == "564177498"
    assert (
        _adam_from_text("https://apps.apple.com/ru/app/sber/id1111111111?l=en")
        == "1111111111"
    )
    assert _adam_from_text("сбербанк") is None
    assert _adam_from_text("12") is None


def test_quota_and_payments() -> None:
    cfg = Config.load()
    cfg.prepare()
    db = Db(cfg.db_path)
    deps = Deps(
        cfg=cfg,
        db=db,
        store=Store(cfg),
        tool=Ipatool(cfg, db),
        sender=FileSender(cfg),
        cards=Cards(cfg),
    )
    tg = 777

    db.user(tg, "tester", "Tester")
    assert deps.may_install(tg) == "free"

    for i in range(cfg.free_per_week):
        assert deps.may_install(tg) == "free", f"итерация {i}"
        db.history_add(tg, "1", "App", "ipa", 0, "ok", "free")

    # лимит выбран — платить нечем
    assert deps.may_install(tg) is None

    # разовая покупка даёт ровно одну установку
    db.add_credits(tg, 1)
    assert deps.may_install(tg) == "credit"
    db.add_credits(tg, -1)
    assert deps.may_install(tg) is None

    # подписка снимает лимит целиком
    until = db.extend_sub(tg, 7)
    assert deps.may_install(tg) == "sub"
    assert db.sub_active(tg) == until

    # платные выдачи не съедают бесплатную неделю
    db.history_add(tg, "1", "App", "ota", 0, "ok", "sub")
    assert db.week_uses(tg) == cfg.free_per_week


def test_accounts_are_personal() -> None:
    cfg = Config.load()
    db = Db(cfg.db_path)
    mine = db.account_add(111, "a@example.com", "pw", "k1")
    theirs = db.account_add(222, "b@example.com", "pw", "k2")
    assert db.account_of(111)["id"] == mine
    assert db.account_of(222)["id"] == theirs
    assert db.account_of(333) is None, "чужой Apple ID подставлять нельзя"
    # рабочий аккаунт всплывает выше сломанного
    second = db.account_add(111, "c@example.com", "pw", "k3")
    db.account_status(second, "ready")
    assert db.account_of(111)["id"] == second


def test_shared_free_quota() -> None:
    """Тот же Apple ID в другом телеграме не должен обнулять недельный лимит."""
    cfg = Config.load()
    db = Db(cfg.db_path)
    deps = Deps(
        cfg=cfg,
        db=db,
        store=Store(cfg),
        tool=Ipatool(cfg, db),
        sender=FileSender(cfg),
        cards=Cards(cfg),
    )
    first, second, stranger = 1001, 1002, 1003
    for tg in (first, second, stranger):
        db.user(tg)

    # пока никто ничего не выкачал, связка ничего не отнимает
    db.link_identity("shared@icloud.com", first)
    db.link_identity("SHARED@icloud.com ", second)  # регистр и пробелы не важны
    db.link_identity("own@icloud.com", stranger)
    assert deps.shared(first) == 2
    assert deps.shared(stranger) == 1
    assert deps.may_install(first) == "free"
    assert deps.may_install(second) == "free"

    # первый выбрал всё бесплатное — второй аккаунт того же человека тоже пуст
    for _ in range(cfg.free_per_week):
        db.history_add(first, "1", "App", "ipa", 0, "ok", "free")
    assert deps.may_install(first) is None
    assert deps.may_install(second) is None, (
        "перелогин в другой телеграм не даёт новых установок"
    )
    assert deps.may_install(stranger) == "free", "чужого человека это не касается"

    # оплаченное остаётся личным: деньги платил конкретный аккаунт
    db.add_credits(second, 1)
    assert deps.may_install(second) == "credit"
    db.add_credits(second, -1)

    # прошла неделя — окно уехало, лимит вернулся обоим
    db.conn.execute("update history set created_at=created_at-?", (8 * 24 * 3600,))
    db.conn.commit()
    assert deps.may_install(first) == "free"
    assert deps.may_install(second) == "free"


def test_cancel_window() -> None:
    job = Job(tg_id=1, title="App")
    job.percent = CANCEL_LIMIT - 1
    assert job.stop() is True
    job = Job(tg_id=1, title="App")
    job.percent = CANCEL_LIMIT
    assert job.stop() is False, "после половины отменять уже нечего"
    assert job.cancelled is False


def test_percent_parser() -> None:
    assert parse_percent("12.5 MB / 250.0 MB") == 5.0
    assert parse_percent("\r 45% |=====   |") == 45.0
    assert parse_percent("1.2 GB / 2.0 GB") == 60.0
    assert parse_percent("ничего похожего") is None


def test_chat_limit() -> None:
    cfg = Config.load()
    db = Db(cfg.db_path)
    sender = FileSender(cfg)
    deps = Deps(
        cfg=cfg,
        db=db,
        store=Store(cfg),
        tool=Ipatool(cfg, db),
        sender=sender,
        cards=Cards(cfg),
    )
    tg = 555
    db.user(tg)
    # без ключей MTProto в чат уходит только то, что глотает Bot API
    assert not sender.ready
    assert deps.chat_limit(tg) == cfg.upload_limit
    # с ключами — весь тариф, но не выше потолка Telegram
    object.__setattr__(cfg, "api_id", 1)
    object.__setattr__(cfg, "api_hash", "x")
    assert sender.ready
    assert deps.chat_limit(tg) == min(cfg.max_bytes(False), MTPROTO_MAX)
    db.extend_sub(tg, 30)
    assert deps.chat_limit(tg) == MTPROTO_MAX, "без Premium дальше 2 ГБ не уедет"
    # Настройка не должна обходить реальный лимит Telegram.
    object.__setattr__(cfg, "mtproto_max_mb", 4000)
    assert deps.chat_limit(tg) == MTPROTO_MAX
    object.__setattr__(cfg, "mtproto_max_mb", 2000)
    object.__setattr__(cfg, "api_id", 0)
    object.__setattr__(cfg, "api_hash", "")


def test_size_limits() -> None:
    cfg = Config.load()
    db = Db(cfg.db_path)
    gb = 1024 * 1024 * 1024
    assert cfg.max_bytes(False) == 2 * gb
    assert cfg.max_bytes(True) == 4 * gb
    tg = 999
    db.user(tg)
    assert cfg.max_bytes(bool(db.sub_active(tg))) == 2 * gb
    db.extend_sub(tg, 30)
    assert cfg.max_bytes(bool(db.sub_active(tg))) == 4 * gb


def test_ipatool_parse() -> None:
    ok = Ipatool._parse(
        '{"apps":[{"id":1}],"count":1,"success":true}', command_failed=False
    )
    assert ok["count"] == 1
    try:
        Ipatool._parse(
            '{"level":"error","error":"failed to get account","success":false}',
            command_failed=True,
        )
    except IpatoolError as exc:
        assert exc.needs_login
    else:  # pragma: no cover
        raise AssertionError("ошибка ipatool должна подниматься наверх")


def test_normalize() -> None:
    app = normalize(
        {
            "trackId": 1,
            "trackName": "T",
            "bundleId": "b",
            "fileSizeBytes": "10",
            "minimumOsVersion": "15.0",
        }
    )
    assert app == {
        "adam_id": "1",
        "bundle_id": "b",
        "title": "T",
        "artist": "",
        "icon": "",
        "version": "",
        "min_ios": "15.0",
        "size": 10,
        "price": "",
        "source": "itunes",
    }


if __name__ == "__main__":
    for name, fn in sorted(globals().items()):
        if name.startswith("test_"):
            fn()
            print(f"ok  {name}")
    print("\nвсё сошлось")
