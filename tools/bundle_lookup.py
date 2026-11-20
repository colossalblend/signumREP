                      
"""Консольный поиск bundle_id по App Store ID или slug каталога Signum."""

from __future__ import annotations

import argparse
import json
import os
import plistlib
import re
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path
from typing import Any
from urllib.error import URLError
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_API = os.getenv("SIGNUM_API_BASE_URL", "https://api.appbank.pw/signum").rstrip("/")
DEFAULT_IPATOOL = Path(os.getenv("SIGNUM_IPATOOL", ROOT / "lib" / "ipatool.exe"))
DEFAULT_PASSPHRASE = os.getenv(
    "SIGNUM_IPATOOL_PASSPHRASE",
    "signum-ipatool-keychain-v1",
)
COUNTRIES = ("ru", "us", "gb", "de", "ua", "kz", "by", "tr", "cn", "fr", "it", "es", "pl", "nl")


def http_json(url: str, timeout: float = 20.0) -> Any:
    request = Request(
        url,
        headers={
            "Accept": "application/json",
            "User-Agent": "SignumBundleLookup/1.0",
        },
    )
    with urlopen(request, timeout=timeout) as response:
        return json.loads(response.read().decode("utf-8"))


def is_app_store_id(value: str) -> bool:
    return value.isdigit() and len(value) >= 5


def itunes_lookup(app_store_id: str) -> dict[str, Any] | None:
    for country in COUNTRIES:
        url = f"https://itunes.apple.com/lookup?id={app_store_id}&country={country}"
        try:
            payload = http_json(url, timeout=12)
        except (URLError, TimeoutError, json.JSONDecodeError):
            continue
        results = payload.get("results") or []
        if not results:
            continue
        item = results[0]
        bundle_id = str(item.get("bundleId") or "").strip()
        if bundle_id:
            return {
                "bundle_id": bundle_id,
                "app_store_id": str(item.get("trackId") or app_store_id),
                "title": str(item.get("trackName") or "").strip(),
                "version": str(item.get("version") or "").strip(),
                "source": f"itunes:{country}",
            }
    return None


def signum_fetch_apps(api_base: str) -> list[dict[str, Any]]:
    payload = http_json(f"{api_base}/main.php?vers=1.0.0&build=1")
    apps = payload.get("apps") or payload.get("data") or []
    if not isinstance(apps, list):
        raise RuntimeError("Signum API: main.php не вернул список приложений")
    return [app for app in apps if isinstance(app, dict) and app.get("id")]


def signum_fetch_app_info(api_base: str, app_id: str) -> dict[str, Any]:
    payload = http_json(f"{api_base}/info.php?id={app_id}&vers=1.0.0&build=1")
    data = payload.get("data", payload)
    if not isinstance(data, dict):
        raise RuntimeError(f"Signum API: info.php?id={app_id} вернул некорректный ответ")
    return data


def signum_lookup_by_slug(api_base: str, slug: str) -> list[dict[str, Any]]:
    info = signum_fetch_app_info(api_base, slug)
    title = str(info.get("title") or slug)
    rows: list[dict[str, Any]] = []
    for version in info.get("versions") or []:
        if not isinstance(version, dict):
            continue
        bundle_id = str(version.get("bundle_id") or "").strip()
        if not bundle_id:
            continue
        rows.append(
            {
                "bundle_id": bundle_id,
                "app_store_id": str(version.get("app_store_id") or "").strip() or None,
                "title": str(version.get("title") or title),
                "version": str(version.get("version") or "").strip() or None,
                "min_ios": str(version.get("min_ios") or "").strip() or None,
                "catalog_id": slug,
                "source": "signum:info",
            }
        )
    if not rows:
        raise RuntimeError(f"В каталоге Signum для «{slug}» нет версий с bundle_id")
    return rows


def signum_lookup_by_app_store_id(api_base: str, app_store_id: str) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for app in signum_fetch_apps(api_base):
        catalog_id = str(app.get("id") or "").strip()
        if not catalog_id:
            continue
        try:
            info = signum_fetch_app_info(api_base, catalog_id)
        except RuntimeError:
            continue
        title = str(info.get("title") or catalog_id)
        for version in info.get("versions") or []:
            if not isinstance(version, dict):
                continue
            if str(version.get("app_store_id") or "").strip() != app_store_id:
                continue
            bundle_id = str(version.get("bundle_id") or "").strip()
            if not bundle_id:
                continue
            rows.append(
                {
                    "bundle_id": bundle_id,
                    "app_store_id": app_store_id,
                    "title": str(version.get("title") or title),
                    "version": str(version.get("version") or "").strip() or None,
                    "min_ios": str(version.get("min_ios") or "").strip() or None,
                    "catalog_id": catalog_id,
                    "source": "signum:catalog",
                }
            )
    return rows


def bundle_from_ipa(ipa_path: Path) -> str | None:
    with zipfile.ZipFile(ipa_path) as archive:
        for name in archive.namelist():
            if not name.startswith("Payload/") or not name.endswith(".app/Info.plist"):
                continue
            with archive.open(name) as handle:
                payload = plistlib.load(handle)
            bundle_id = payload.get("CFBundleIdentifier")
            if isinstance(bundle_id, str) and bundle_id.strip():
                return bundle_id.strip()
    return None


def ipatool_lookup(app_store_id: str, ipatool: Path, passphrase: str) -> dict[str, Any] | None:
    if not ipatool.exists():
        return None

    with tempfile.TemporaryDirectory(prefix="signum-bundle-") as temp_dir:
        output_dir = Path(temp_dir)
        command = [
            str(ipatool),
            "download",
            "-i",
            app_store_id,
            "-o",
            str(output_dir),
            "--format",
            "json",
            "--non-interactive",
            "--keychain-passphrase",
            passphrase,
            "--purchase",
        ]
        try:
            completed = subprocess.run(
                command,
                capture_output=True,
                text=True,
                timeout=900,
                check=False,
            )
        except subprocess.TimeoutExpired:
            raise RuntimeError("ipatool: превышено время ожидания загрузки IPA")

        stderr = completed.stderr.strip()
        stdout = completed.stdout.strip()
        if completed.returncode != 0 and not list(output_dir.glob("*.ipa")):
            detail = stderr or stdout or f"код {completed.returncode}"
            raise RuntimeError(f"ipatool download не удался: {detail}")

        ipa_files = sorted(output_dir.glob("*.ipa"), key=lambda path: path.stat().st_mtime, reverse=True)
        if not ipa_files:
            raise RuntimeError("ipatool завершился без IPA-файла")

        bundle_id = bundle_from_ipa(ipa_files[0])
        if not bundle_id:
            raise RuntimeError("Не удалось прочитать CFBundleIdentifier из IPA")

        version = None
        match = re.search(r'"displayVersion"\s*:\s*"([^"]+)"', stdout)
        if match:
            version = match.group(1)

        return {
            "bundle_id": bundle_id,
            "app_store_id": app_store_id,
            "title": ipa_files[0].stem,
            "version": version,
            "source": "ipatool:download",
        }


def print_rows(rows: list[dict[str, Any]], *, as_json: bool) -> None:
    if as_json:
        print(json.dumps(rows, ensure_ascii=False, indent=2))
        return

    for index, row in enumerate(rows, start=1):
        prefix = f"[{index}] " if len(rows) > 1 else ""
        print(f"{prefix}bundle_id: {row['bundle_id']}")
        if row.get("app_store_id"):
            print(f"{' ' * len(prefix)}app_store_id: {row['app_store_id']}")
        if row.get("catalog_id"):
            print(f"{' ' * len(prefix)}catalog_id: {row['catalog_id']}")
        if row.get("title"):
            print(f"{' ' * len(prefix)}title: {row['title']}")
        if row.get("version"):
            print(f"{' ' * len(prefix)}version: {row['version']}")
        if row.get("min_ios"):
            print(f"{' ' * len(prefix)}min_ios: {row['min_ios']}")
        print(f"{' ' * len(prefix)}source: {row['source']}")
        if index < len(rows):
            print()


def resolve(
    identifier: str,
    *,
    api_base: str,
    use_ipatool: bool,
    ipatool: Path,
    passphrase: str,
) -> list[dict[str, Any]]:
    identifier = identifier.strip()
    if not identifier:
        raise RuntimeError("Укажите id приложения")

    if is_app_store_id(identifier):
        rows = signum_lookup_by_app_store_id(api_base, identifier)
        if rows:
            return rows

        itunes = itunes_lookup(identifier)
        if itunes:
            return [itunes]

        if use_ipatool:
            ipatool_row = ipatool_lookup(identifier, ipatool, passphrase)
            if ipatool_row:
                return [ipatool_row]

        raise RuntimeError(
            "bundle_id не найден. Приложение удалено из публичного каталога — "
            "добавьте в Signum API или запустите с --ipatool (нужен вход в Apple ID в Signum)."
        )

    return signum_lookup_by_slug(api_base, identifier)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Получить bundle_id для Signum API по App Store ID или slug каталога (sber, alfa).",
    )
    parser.add_argument(
        "id",
        nargs="?",
        help="App Store ID (цифры) или slug из каталога Signum",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Вывести результат в JSON",
    )
    parser.add_argument(
        "--ipatool",
        action="store_true",
        help="Если публичный поиск не помог — скачать IPA через ipatool и прочитать bundle_id",
    )
    parser.add_argument(
        "--ipatool-path",
        default=str(DEFAULT_IPATOOL),
        help=f"Путь к ipatool (по умолчанию: {DEFAULT_IPATOOL})",
    )
    parser.add_argument(
        "--passphrase",
        default=DEFAULT_PASSPHRASE,
        help="Ключ ipatool (по умолчанию встроенный ключ Signum)",
    )
    parser.add_argument(
        "--api",
        default=DEFAULT_API,
        help=f"Базовый URL Signum API (по умолчанию: {DEFAULT_API})",
    )
    return parser


def pause_before_exit() -> None:
    if not sys.stdin.isatty():
        return
    try:
        input("\nНажмите Enter для выхода...")
    except (EOFError, KeyboardInterrupt):
        pass


def run_interactive(
    *,
    api_base: str,
    use_ipatool: bool,
    ipatool: Path,
    passphrase: str,
    as_json: bool,
) -> int:
    print("Signum — поиск bundle_id для API")
    print("Введите App Store ID (цифры) или slug каталога: sber, alfa")
    print("Пустой ввод — выход.\n")

    while True:
        try:
            value = input("Введите id: ").strip()
        except (EOFError, KeyboardInterrupt):
            print()
            return 0

        if not value:
            return 0

        print()
        try:
            rows = resolve(
                value,
                api_base=api_base,
                use_ipatool=use_ipatool,
                ipatool=ipatool,
                passphrase=passphrase,
            )
            print_rows(rows, as_json=as_json)
        except RuntimeError as error:
            print(f"Ошибка: {error}")
        print()


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    api_base = str(args.api).rstrip("/")
    ipatool = Path(args.ipatool_path)

    if args.id:
        try:
            rows = resolve(
                args.id,
                api_base=api_base,
                use_ipatool=args.ipatool,
                ipatool=ipatool,
                passphrase=args.passphrase,
            )
            print_rows(rows, as_json=args.json)
            return 0
        except RuntimeError as error:
            print(f"Ошибка: {error}", file=sys.stderr)
            return 1

    code = run_interactive(
        api_base=api_base,
        use_ipatool=args.ipatool,
        ipatool=ipatool,
        passphrase=args.passphrase,
        as_json=args.json,
    )
    pause_before_exit()
    return code


if __name__ == "__main__":
    raise SystemExit(main())