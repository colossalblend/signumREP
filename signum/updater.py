from __future__ import annotations

import asyncio
from dataclasses import dataclass
from typing import Awaitable, Callable

from signum.api import ApiClient
from signum.device import DeviceManager
from signum.ipatool import IpaToolManager
from signum.models import AppInfo, AppSummary
from signum.storage import CredentialStore
from signum.utils import compare_versions, looks_like_version


@dataclass(frozen=True)
class UpdateCandidate:
    title: str
    info: AppInfo
    installed_version: str
    remote_version: str


UpdateCallback = Callable[[UpdateCandidate], Awaitable[None]]


class SmartAutoUpdater:
    def __init__(
        self,
        *,
        api: ApiClient,
        device: DeviceManager,
        ipatool: IpaToolManager,
        store: CredentialStore,
        on_update_found: UpdateCallback,
        interval_seconds: int = 600,
    ) -> None:
        self.api = api
        self.device = device
        self.ipatool = ipatool
        self.store = store
        self.on_update_found = on_update_found
        self.interval_seconds = interval_seconds
        self._task: asyncio.Task[None] | None = None
        self._stopped = asyncio.Event()
        self._notified: set[tuple[str, str]] = set()

    async def start(self) -> None:
        if self._task and not self._task.done():
            return
        self._stopped.clear()
        self._task = asyncio.create_task(self._run(), name="signum-smart-auto-update")

    async def stop(self) -> None:
        self._stopped.set()
        if self._task:
            self._task.cancel()
            try:
                await self._task
            except asyncio.CancelledError:
                pass

    async def capture_known_apps(self, apps: list[AppSummary]) -> None:
        if not self.store.get_known_app_ids():
            self.store.remember_known_app_ids({app.id for app in apps})

    async def _run(self) -> None:
        await asyncio.sleep(15)
        while not self._stopped.is_set():
            try:
                await self.check_once()
            except Exception:
                pass
            try:
                await asyncio.wait_for(self._stopped.wait(), timeout=self.interval_seconds)
            except asyncio.TimeoutError:
                continue

    async def check_once(self) -> None:
        apps = await self.api.fetch_apps()
        known_ids = self.store.get_known_app_ids()
        current_ids = {app.id for app in apps}

        for app in apps:
            if app.id not in known_ids:
                await self._purchase_new_app_if_possible(app)

        self.store.remember_known_app_ids(current_ids)
        installed = await self.device.list_installed_apps()
        for app in apps:
            info = await self.api.fetch_app_info(app.id)
            if not info.bundle_id or not info.version or not looks_like_version(info.version):
                continue
            installed_app = installed.get(info.bundle_id)
            if not installed_app or not installed_app.version or not looks_like_version(installed_app.version):
                continue
            if compare_versions(info.version, installed_app.version) > 0:
                key = (info.bundle_id, info.version)
                if key in self._notified:
                    continue
                self._notified.add(key)
                await self.on_update_found(
                    UpdateCandidate(
                        title=info.title or app.title,
                        info=info,
                        installed_version=installed_app.version,
                        remote_version=info.version,
                    )
                )

    async def _purchase_new_app_if_possible(self, app: AppSummary) -> None:
        if not self.store.get_auth_session():
            return
        info = await self.api.fetch_app_info(app.id)
        if not info.bundle_id or not info.in_store or self.store.is_purchased(info.bundle_id):
            return
        try:
            await self.ipatool.purchase(info.bundle_id)
        except Exception:
            return
