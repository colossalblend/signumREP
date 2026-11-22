from __future__ import annotations

import re
import asyncio
import time
import json
import secrets
from typing import Any

import httpx
from cachetools import TTLCache

from .config import Config

ITUNES = "https://itunes.apple.com"


class Store:
    """Метаданные приложений: каталог Signum + публичный iTunes API.

    iTunes отдаёт иконку, размер и минимальную iOS без входа в Apple ID,
    поэтому аккаунты тратятся только на то, ради чего они нужны, — покупки и
    скачивание.
    """

    def __init__(self, cfg: Config) -> None:
        self.cfg = cfg
        self._cache: TTLCache = TTLCache(maxsize=256, ttl=1800)
        self._inflight: dict[str, asyncio.Task] = {}
        self._slots = asyncio.Semaphore(8)
        self._client: httpx.AsyncClient | None = None

    async def _get(self, url: str, params: dict[str, Any], *, ttl: float = 600) -> Any:
        key = f"{url}?{sorted(params.items())}"
        cached = self._cache.get(key)
        if cached and cached[1] > time.monotonic():
            return cached[0]
        if key not in self._inflight:
            if len(self._inflight) >= 128:
                raise RuntimeError("Метаданные временно заняты — повторите чуть позже")
            task = asyncio.create_task(self._fetch(key, url, params, ttl))
            self._inflight[key] = task
            task.add_done_callback(lambda item: self._finished(key, item))
        return await asyncio.shield(self._inflight[key])

    def _finished(self, key, task):
        self._inflight.pop(key, None)
        if not task.cancelled():
            task.exception()

    async def _fetch(self, key, url, params, ttl):
        async with self._slots:
            if self._client is None:
                self._client = httpx.AsyncClient(
                    timeout=httpx.Timeout(12, connect=3),
                    follow_redirects=True,
                    trust_env=False,
                    limits=httpx.Limits(max_connections=8, max_keepalive_connections=8),
                )
            headers = (
                {
                    "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36",
                    "Accept": "application/json, text/plain, */*",
                    "Origin": "https://appmagic.rocks",
                    "Referer": "https://appmagic.rocks/",
                }
                if url.startswith("https://appmagic.rocks/")
                else None
            )
            if getattr(self.cfg, "bridge_enabled", False) and url in (
                f"{self.cfg.api_base_url}/main.php",
                f"{self.cfg.api_base_url}/info.php",
            ):
                from .bridge import BridgeWorker

                worker = BridgeWorker(self.cfg)
                operation = "catalog_app" if url.endswith("/info.php") else "catalog"
                fields = (
                    {"app_id": str(params["id"])} if operation == "catalog_app" else {}
                )
                result = await worker.exchange(self._client, operation, **fields)
                data = result["data"]
            else:
                response = await self._client.get(url, params=params, headers=headers)
                response.raise_for_status()
                data = response.json()
            if url == f"{self.cfg.api_base_url}/main.php" and self._valid_catalog(data):
                await asyncio.to_thread(self._save_catalog, data)
        self._cache[key] = (data, time.monotonic() + ttl)
        return data

    async def close(self):
        tasks = list(self._inflight.values())
        for task in tasks:
            task.cancel()
        await asyncio.gather(*tasks, return_exceptions=True)
        if self._client:
            await self._client.aclose()

    # --- каталог Signum ---------------------------------------------------
    @staticmethod
    def _valid_catalog(data):
        return (
            isinstance(data, dict)
            and isinstance(data.get("groups"), list)
            and isinstance(data.get("apps"), list)
        )

    def _save_catalog(self, data):
        if not getattr(self.cfg, "data_dir", None):
            return
        path = self.cfg.data_dir / "store-catalog.json"
        try:
            pending = path.with_name(path.name + "." + secrets.token_hex(4) + ".part")
            pending.write_text(
                json.dumps(
                    {
                        "time": time.time(),
                        "source": self.cfg.api_base_url,
                        "data": data,
                    },
                    ensure_ascii=False,
                ),
                encoding="utf-8",
            )
            pending.replace(path)
        except OSError:
            pass
        finally:
            pending.unlink(missing_ok=True)

    def _saved_catalog(self):
        if not getattr(self.cfg, "data_dir", None):
            return None
        try:
            path = self.cfg.data_dir / "store-catalog.json"
            if path.stat().st_size > 2097152:
                return None
            saved = json.loads(path.read_text(encoding="utf-8"))
            if (
                saved["source"] == self.cfg.api_base_url
                and 0 <= time.time() - saved["time"] <= 86400
                and self._valid_catalog(saved["data"])
            ):
                return saved["data"]
        except (OSError, ValueError, KeyError, TypeError):
            pass
        return None

    async def catalog(self) -> dict[str, Any]:
        try:
            data = await self._get(
                f"{self.cfg.api_base_url}/main.php", {"vers": "1.1.3", "build": "26"}
            )
        except httpx.HTTPError:
            data = await asyncio.to_thread(self._saved_catalog)
            if data is None:
                raise
        return data if isinstance(data, dict) else {}

    async def catalog_app(self, app_id: str) -> dict[str, Any]:
        data = await self._get(
            f"{self.cfg.api_base_url}/info.php",
            {"vers": "1.1.3", "build": "26", "id": app_id},
        )
        payload = data.get("data") if isinstance(data, dict) else None
        return payload if isinstance(payload, dict) else {}

    # --- iTunes -----------------------------------------------------------
    async def search(
        self, term: str, limit: int = 12, country: str = "ru"
    ) -> list[dict[str, Any]]:
        if re.fullmatch(r"[A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]+){2,}", term.strip()):
            for region in dict.fromkeys((country, "us")):
                data = await self._get(
                    f"{ITUNES}/lookup", {"bundleId": term.strip(), "country": region}
                )
                if isinstance(data, dict) and data.get("results"):
                    return [normalize(item) for item in data["results"]]
            return []
        batches = await asyncio.gather(
            *(
                self._get(
                    "https://appmagic.rocks/api/v2/search",
                    {"name": query, "limit": 100},
                    ttl=1800,
                )
                for query in search_variants(term.strip()[:200])
            ),
            return_exceptions=True,
        )
        results = [
            appmagic_app(item)
            for data in batches
            if isinstance(data, dict)
            for item in data.get("applications", [])
        ]
        if not results and all(isinstance(data, Exception) for data in batches):
            raise batches[0]
        seen = set()
        output = []
        for item in results:
            if item and item["adam_id"] not in seen:
                seen.add(item["adam_id"])
                output.append(item)
        return output[: min(100, limit)]

    async def developers(self, term: str):
        batches = await asyncio.gather(
            *(
                self._get(
                    f"{ITUNES}/search",
                    {
                        "term": term[:200],
                        "entity": "software",
                        "attribute": "softwareDeveloper",
                        "country": country,
                        "limit": 60,
                    },
                )
                for country in ("ru", "us")
            )
        )
        found = {}
        seen = set()
        for batch in batches:
            for item in batch.get("results", []):
                artist = str(item.get("artistId") or "")
                if (
                    not artist.isdigit()
                    or not item.get("artistName")
                    or item.get("trackId") in seen
                ):
                    continue
                seen.add(item.get("trackId"))
                row = found.setdefault(
                    artist, {"artist_id": artist, "name": item["artistName"], "apps": 0}
                )
                row["apps"] += 1
        return sorted(found.values(), key=lambda row: row["apps"], reverse=True)[:20]

    async def developer_apps(self, artist_id: str):
        if not re.fullmatch(r"[0-9]{1,20}", artist_id):
            return []
        batches = await asyncio.gather(
            *(
                self._get(
                    f"{ITUNES}/lookup",
                    {
                        "id": artist_id,
                        "entity": "software",
                        "country": country,
                        "limit": 200,
                    },
                )
                for country in ("ru", "us")
            )
        )
        found = {
            str(item["trackId"]): normalize(item)
            for batch in batches
            for item in batch.get("results", [])
            if item.get("trackId")
        }
        return sorted(found.values(), key=lambda row: row["title"].casefold())

    async def lookup(self, adam_id: str, country: str = "ru") -> dict[str, Any] | None:
        for store in (country, "us"):
            data = await self._get(
                f"{ITUNES}/lookup", {"id": str(adam_id), "country": store}, ttl=1800
            )
            results = data.get("results") if isinstance(data, dict) else None
            if results:
                return normalize(results[0])
        results = await self.search(str(adam_id), limit=100)
        return next((row for row in results if row["adam_id"] == str(adam_id)), None)


def appmagic_app(item):
    identifiers = item.get("store_ids") or []
    adam = next(
        (
            str(value)[2:]
            for prefix in ("2_", "3_")
            for value in identifiers
            if str(value).startswith(prefix) and str(value)[2:].isdigit()
        ),
        "",
    )
    names = item.get("names") or []
    title = item.get("name") or next(
        (str(value) for value in names if isinstance(value, str)), ""
    )
    if not adam or not title:
        return None
    return {
        "adam_id": adam,
        "title": title,
        "artist": (item.get("unitedPublisher") or {}).get("name")
        or item.get("publisher_name")
        or "",
        "icon": item.get("icon_url") or "",
        "bundle_id": "",
        "version": "",
        "min_ios": "",
        "size": 0,
        "price": "",
        "source": "appmagic",
        "removed": bool(item.get("removed"))
        or (
            any(value in (2, 3) for value in item.get("removed_stores", []))
            and not any(value in (2, 3) for value in item.get("store", []))
        ),
    }


def search_variants(query):
    if not re.search(r"[а-яё]", query, re.I):
        return [query]
    letters = "абвгдеёжзийклмнопрстуфхцчшщъыьэюя"
    latin = "a b v g d e e zh z i y k l m n o p r s t u f h ts ch sh sch _ y _ e yu ya".split()
    mapping = dict(zip(letters, latin))
    transliterated = "".join(mapping.get(char, char) for char in query.casefold())
    compact = re.sub(r"[^a-z0-9]", "", transliterated)
    return [query, compact] if compact and compact != query else [query]


def normalize(item: dict[str, Any]) -> dict[str, Any]:
    return {
        "adam_id": str(item.get("trackId") or ""),
        "bundle_id": item.get("bundleId") or "",
        "title": item.get("trackName") or item.get("trackCensoredName") or "",
        "artist": item.get("artistName") or "",
        "icon": item.get("artworkUrl512") or item.get("artworkUrl100") or "",
        "version": item.get("version") or "",
        "min_ios": item.get("minimumOsVersion") or "",
        "size": int(item.get("fileSizeBytes") or 0),
        "price": item.get("formattedPrice") or "",
        "source": "itunes",
    }
