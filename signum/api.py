from __future__ import annotations

from typing import Any

import httpx

from signum.models import AppInfo, AppSummary, OtaUpdate
from signum.utils import compare_versions


class ApiClient:
    def __init__(
        self,
        base_url: str,
        *,
        app_version: str,
        app_build: str,
        devkey: str,
        timeout: float = 15,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self.app_version = app_version
        self.app_build = app_build
        self.devkey = devkey
        self.timeout = timeout

    async def fetch_apps(self) -> list[AppSummary]:
        data = await self._get_json("/main.php")
        if isinstance(data, dict):
            raw_apps = data.get("apps") or data.get("data") or []
        else:
            raw_apps = data
        if not isinstance(raw_apps, list):
            raise ValueError("main.php must return an app list or an object with apps/data")
        apps = [AppSummary.from_api(item) for item in raw_apps if isinstance(item, dict) and item.get("id") is not None]
        return [app for app in apps if app.active]

    async def fetch_app_info(self, app_id: str) -> AppInfo:
        data = await self._get_json("/info.php", params={"id": app_id})
        if isinstance(data, dict) and isinstance(data.get("data"), dict):
            data = data["data"]
        if not isinstance(data, dict):
            raise ValueError("info.php must return a JSON object")
        return AppInfo.from_api(app_id, data)

    async def fetch_ota_update(self) -> OtaUpdate:
        data = await self._get_json("/ota.php")
        if isinstance(data, dict) and isinstance(data.get("data"), dict):
            data = data["data"]
        if not isinstance(data, dict):
            raise ValueError("ota.php must return a JSON object")

        update = OtaUpdate.from_api(data)
        if not update.has_update:
            return update
        if update.version and compare_versions(update.version, self.app_version) <= 0:
            if not update.build:
                return OtaUpdate(False, update.version, update.build, update.download_url, update.notes, update.mandatory)
        if update.build and _compare_builds(update.build, self.app_build) <= 0:
            if not update.version or compare_versions(update.version, self.app_version) <= 0:
                return OtaUpdate(False, update.version, update.build, update.download_url, update.notes, update.mandatory)
        return update

    async def _get_json(self, path: str, *, params: dict[str, str] | None = None) -> Any:
        url = f"{self.base_url}{path}"
        query = {
            "vers": self.app_version,
            "build": self.app_build,
            "devkey": self.devkey,
        }
        if params:
            query.update(params)
        async with httpx.AsyncClient(timeout=self.timeout, follow_redirects=True) as client:
            response = await client.get(url, params=query)
            response.raise_for_status()
            return response.json()


def _compare_builds(left: str, right: str) -> int:
    try:
        left_int = int(left)
        right_int = int(right)
    except ValueError:
        return compare_versions(left, right)
    return (left_int > right_int) - (left_int < right_int)
