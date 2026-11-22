from __future__ import annotations

import asyncio
import html
import plistlib
import shutil
import secrets
import zipfile
from urllib.parse import quote
from pathlib import Path
from typing import Any

from aiohttp import web
from cachetools import TTLCache

from .config import Config
from .db import Db, now
from .rate_limits import take_request


class LeasedFileResponse(web.FileResponse):
    def __init__(self, path, *, release, headers):
        super().__init__(path, headers=headers)
        self.release = release

    async def prepare(self, request):
        try:
            return await super().prepare(request)
        finally:
            self.release()


def manifest(
    url: str, bundle_id: str, version: str, title: str, icon: str = ""
) -> bytes:
    """OTA-манифест для itms-services."""
    # Только сам пакет: иконки iOS тянет отдельно во время установки, и любой
    # сбой на их адресе валит всю установку. Рабочие манифесты банков их тоже
    # не содержат.
    assets = [{"kind": "software-package", "url": url}]
    payload = {
        "items": [
            {
                "assets": assets,
                "metadata": {
                    "bundle-identifier": bundle_id or "com.signum.app",
                    "bundle-version": version or "1.0",
                    "kind": "software",
                    "title": title or "App",
                },
            }
        ]
    }
    return plistlib.dumps(payload)


def installation_links(base_url: str, token: str) -> dict[str, str]:
    manifest_url = f"{base_url}/ota/{token}.plist"
    return {
        "install_url": f"{base_url}/i/{token}",
        "manifest_url": manifest_url,
        "itms_services_url": "itms-services://?action=download-manifest&url="
        + quote(manifest_url, safe=""),
    }


def ipa_facts(path: Path) -> dict[str, Any]:
    """Читает IPA: настоящий bundle id, версию и шифрование FairPlay.

    Версию и идентификатор берём из самой сборки, а не из каталога: в манифест
    должно попасть то, что реально лежит внутри, иначе iOS отвергнет установку.
    Папка SC_Info появляется только у покупок из App Store — такие сборки Apple
    шифрует под учётную запись, и по воздуху они не ставятся ни у кого.
    """
    facts: dict[str, Any] = {"bundle_id": "", "version": "", "encrypted": False}
    try:
        with zipfile.ZipFile(path) as archive:
            names = archive.namelist()
            info = next(
                (
                    name
                    for name in names
                    if name.startswith("Payload/")
                    and name.endswith(".app/Info.plist")
                    and name.count("/") == 2
                ),
                None,
            )
            facts["encrypted"] = any("/SC_Info/" in name for name in names)
            if info:
                plist = plistlib.loads(archive.read(info))
                facts["bundle_id"] = str(plist.get("CFBundleIdentifier") or "")
                facts["version"] = str(
                    plist.get("CFBundleShortVersionString")
                    or plist.get("CFBundleVersion")
                    or ""
                )
    except Exception:  # noqa: BLE001 - битый архив разберём выше по стеку
        pass
    return facts


def repack(path: Path) -> Path:
    """Пересобирает IPA, иначе iPhone отказывается ставить его по воздуху.

    ipatool дописывает `sinf` и `iTunesMetadata.plist` в конец готового архива,
    и получается zip, который сам себе противоречит: карта Apple в META-INF
    описывает состояние до дозаписи. Через `itms-services` такой файл скачивается
    целиком и отваливается на установке. Пишем архив заново одним проходом —
    метаданные первыми, устаревшую карту выбрасываем. Ровно это делает iMazing
    при экспорте из медиатеки, после которого установка проходит.
    """
    tmp = path.with_suffix(".repack")
    with zipfile.ZipFile(path) as zin:
        items = [item for item in zin.infolist() if not item.is_dir()]
        items.sort(
            key=lambda item: (
                0 if item.filename == "iTunesMetadata.plist" else 1,
                item.filename,
            )
        )
        with zipfile.ZipFile(tmp, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as zout:
            for item in items:
                if item.filename.startswith("META-INF/"):
                    continue
                info = zipfile.ZipInfo(item.filename, date_time=item.date_time)
                info.external_attr = item.external_attr
                info.compress_type = zipfile.ZIP_DEFLATED
                with (
                    zin.open(item) as source,
                    zout.open(info, "w", force_zip64=True) as target,
                ):
                    shutil.copyfileobj(source, target, length=1024 * 1024)
    shutil.move(str(tmp), path)
    return path


async def sign(cfg: Config, ipa: Path) -> bool:
    """Подписывает IPA через zsign, если он настроен.

    App Store-сборки без подписи ставятся по воздуху только на устройство с тем
    же Apple ID, поэтому подпись — это разница между «работает у всех» и
    «работает иногда». Не настроено — просто отдаём как есть.
    """
    conf = cfg.zsign
    if not conf.get("bin") or not conf.get("cert"):
        return False
    args = [conf["bin"], "-z", "5"]
    if conf.get("prov"):
        args += ["-m", conf["prov"]]
    if conf["cert"].endswith(".p12"):
        args += ["-k", conf["cert"]]
        if conf.get("password"):
            args += ["-p", conf["password"]]
    else:
        args += ["-c", conf["cert"]]
        if conf.get("key"):
            args += ["-k", conf["key"]]
    args.append(str(ipa))
    proc = await asyncio.create_subprocess_exec(
        *args, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.STDOUT
    )
    out, _ = await proc.communicate()
    if proc.returncode != 0:
        raise RuntimeError(
            (out or b"").decode("utf-8", "replace")[-400:] or "zsign не справился"
        )
    return True


PAGE = """<!doctype html>
<html lang="ru"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover">
<title>{title} — установка</title>
<style>
:root{{color-scheme:dark}}
*{{box-sizing:border-box}}
body{{margin:0;min-height:100vh;display:grid;place-items:center;padding:24px;
font:16px/1.5 -apple-system,BlinkMacSystemFont,"SF Pro Text",system-ui,sans-serif;
background:radial-gradient(120% 80% at 50% 0%,#1d2340 0%,#0b0d16 60%);color:#eef1ff}}
.card{{width:100%;max-width:380px;background:rgba(255,255,255,.05);border:1px solid rgba(255,255,255,.09);
border-radius:26px;padding:28px 22px;text-align:center;backdrop-filter:blur(18px);
box-shadow:0 24px 60px rgba(0,0,0,.45)}}
img{{width:104px;height:104px;border-radius:24px;box-shadow:0 12px 30px rgba(0,0,0,.5)}}
h1{{font-size:21px;margin:16px 0 4px}}
.sub{{color:#9aa3c7;font-size:13px;margin:0}}
.meta{{margin:18px 0 22px;display:flex;gap:8px;justify-content:center;flex-wrap:wrap}}
.meta span{{background:rgba(255,255,255,.07);border-radius:999px;padding:5px 12px;font-size:12px;color:#c9cfeb}}
a.btn{{display:block;padding:15px;border-radius:16px;text-decoration:none;font-weight:600;
background:linear-gradient(135deg,#6d5cff,#3aa0ff);color:#fff}}
a.alt{{background:none;border:1px solid rgba(255,255,255,.16);color:#c9cfeb;margin-top:10px;font-weight:500}}
.hint{{margin:18px 0 0;font-size:12px;color:#79809f}}
</style></head><body><div class="card">
{icon}
<h1>{title}</h1><p class="sub">{artist}</p>
<div class="meta"><span>Версия {version}</span><span>{size}</span></div>
<a class="btn" href="itms-services://?action=download-manifest&amp;url={manifest_url}">Установить на iPhone</a>
<p class="hint">Открывайте эту страницу в Safari на iPhone.<br>Ссылка живёт {ttl} ч.</p>
</div></body></html>"""


def human_size(size: int) -> str:
    value = float(size or 0)
    for unit in ("Б", "КБ", "МБ", "ГБ"):
        if value < 1024 or unit == "ГБ":
            return (
                f"{value:.0f} {unit}" if unit in ("Б", "КБ") else f"{value:.1f} {unit}"
            )
        value /= 1024
    return f"{value:.1f} ГБ"


def build_app(cfg: Config, db: Db, customer_api=None) -> web.Application:
    app = web.Application(client_max_size=16 * 1024)
    if cfg.bridge_enabled:
        from .install_feedback import register_events

        register_events(app, cfg, db)
    file_requests = TTLCache(maxsize=4096, ttl=60)
    active_files: dict[str, int] = {}

    def row_or_404(token: str):
        row = db.file(token)
        if (
            row is None
            or int(row["expires_at"]) <= now()
            or not Path(row["path"]).exists()
        ):
            raise web.HTTPNotFound(text="Ссылка устарела")
        return row

    async def ipa(request: web.Request) -> web.StreamResponse:
        peer = request.remote or "unknown"
        if (
            not take_request(file_requests, peer, 60)
            or active_files.get(peer, 0) >= 3
            or sum(active_files.values()) >= 12
        ):
            raise web.HTTPTooManyRequests(
                text="Повторите скачивание чуть позже", headers={"Retry-After": "60"}
            )
        row = row_or_404(request.match_info["token"])

        def release():
            left = active_files[peer] - 1
            if left:
                active_files[peer] = left
            else:
                active_files.pop(peer, None)
            if (
                response.status in (200, 206)
                and request.method == "GET"
                and "iphone" in request.headers.get("User-Agent", "").lower()
            ):
                from .install_feedback import record_event

                record_event(
                    db,
                    {
                        "id": secrets.token_hex(16),
                        "token": row["token"],
                        "kind": "iphone_download_finished",
                    },
                )

        response = LeasedFileResponse(
            Path(row["path"]),
            release=release,
            headers={
                "Content-Disposition": f'attachment; filename="{row["name"]}"',
                "Cache-Control": "private, no-store",
                "X-Content-Type-Options": "nosniff",
                "Referrer-Policy": "no-referrer",
            },
        )
        active_files[peer] = active_files.get(peer, 0) + 1
        return response

    async def plist(request: web.Request) -> web.Response:
        row = row_or_404(request.match_info["token"])
        body = manifest(
            f"{cfg.public_base_url}/f/{row['token']}.ipa",
            row["bundle_id"],
            row["version"],
            row["title"],
        )
        return web.Response(body=body, content_type="application/xml")

    async def page(request: web.Request) -> web.Response:
        row = row_or_404(request.match_info["token"])
        app_row = db.app(row["adam_id"])
        icon_url = app_row["icon"] if app_row else ""
        return web.Response(
            text=PAGE.format(
                title=html.escape(row["title"] or "Приложение"),
                artist=html.escape(app_row["artist"] if app_row else ""),
                version=html.escape(row["version"] or "—"),
                size=human_size(int(row["size"] or 0)),
                icon=f'<img src="{html.escape(icon_url)}" alt="">' if icon_url else "",
                manifest_url=html.escape(
                    f"{cfg.public_base_url}/ota/{row['token']}.plist"
                ),
                ttl=int(row["ttl_hours"]),
            ),
            content_type="text/html",
        )

    async def cert(_request: web.Request) -> web.StreamResponse:
        """Отдаёт сертификат, чтобы iPhone мог его доверить."""
        path = cfg.data_dir / "local-cert.pem"
        if not path.exists():
            raise web.HTTPNotFound(text="Локальный HTTPS не включён")
        return web.FileResponse(
            path,
            headers={
                "Content-Type": "application/x-x509-ca-cert",
                "Content-Disposition": 'attachment; filename="signum-local.crt"',
            },
        )

    app.router.add_get("/cert", cert)
    app.router.add_get("/f/{token}.ipa", ipa)
    app.router.add_get("/ota/{token}.plist", plist)
    app.router.add_get("/i/{token}", page)

    async def health(_request):
        return web.json_response({"ok": True})

    app.router.add_get("/healthz", health)
    if customer_api:
        customer_api.register(app)
    return app


async def serve(cfg: Config, db: Db, customer_api=None, *, in_process=False) -> web.AppRunner:
    from .api_lease import ApiLease

    lease = ApiLease(cfg.data_dir / "api-server.lock")
    app = build_app(cfg, db, customer_api)

    async def release(_app):
        lease.close()

    app.on_cleanup.append(release)
    runner = web.AppRunner(
        app, keepalive_timeout=15, max_field_size=4096, max_line_size=4096
    )
    try:
        await runner.setup()
        if not in_process:
            await web.TCPSite(runner, cfg.http_host, cfg.http_port).start()
        if not in_process and cfg.tls_port and cfg.tls_hosts:
            from .tls import context, ensure_cert

            cert_path, key_path = ensure_cert(cfg.data_dir, cfg.tls_hosts)
            await web.TCPSite(
                runner,
                cfg.http_host,
                cfg.tls_port,
                ssl_context=context(cert_path, key_path),
            ).start()
    except BaseException:
        await runner.cleanup()
        lease.close()
        raise
    return runner


def purge(cfg: Config, db: Db, everything: bool = False) -> tuple[int, int]:
    """Стирает файлы с диска и из базы. Возвращает сколько штук и байт ушло."""
    rows = db.files_all() if everything else db.files_expired()
    count = size = 0
    for row in rows:
        path = Path(row["path"])
        if path.parent.parent == cfg.files_dir:
            shutil.rmtree(path.parent, ignore_errors=True)
        else:
            path.unlink(missing_ok=True)
        db.file_delete(row["token"])
        count += 1
        size += int(row["size"] or 0)
    return count, size


async def cleaner(cfg: Config, db: Db, period: int = 60) -> None:
    """Убирает просроченные файлы: диск не резиновый."""
    while True:
        try:
            purge(cfg, db)
        except Exception:
            pass
        await asyncio.sleep(period)


def expiry(cfg: Config, paid_with: str) -> int:
    return now() + cfg.link_hours(paid_with) * 3600


def migrate_links(cfg: Config, db: Db) -> int:
    db._run(
        "update files set ttl_hours=?,expires_at=min(expires_at,created_at+?) where paid_with='free' and ttl_hours>?",
        (
            cfg.link_ttl_free_hours,
            cfg.link_ttl_free_hours * 3600,
            cfg.link_ttl_free_hours,
        ),
    )
    rows = db.all("select * from files where ttl_hours=0")
    for row in rows:
        history = db.one(
            "select paid from history where tg_id=? and adam_id=? and account_id=? "
            "and status='ok' and created_at>=? and created_at<=? order by created_at, id limit 1",
            (
                row["tg_id"],
                row["adam_id"],
                row["account_id"],
                row["created_at"],
                row["created_at"] + 300,
            ),
        )
        paid = row["paid_with"] or (history["paid"] if history else "free")
        ttl = cfg.link_hours(paid)
        db._run(
            "update files set paid_with=?, ttl_hours=?, expires_at=? where token=?",
            (paid, ttl, int(row["created_at"]) + ttl * 3600, row["token"]),
        )
    return len(rows)
