from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
from typing import Any


@dataclass(frozen=True)
class AppSummary:
    id: str
    title: str
    bundle_id: str | None = None
    icon_url: str | None = None
    badge: str | None = None
    version: str | None = None
    active: bool = True
    in_store: bool = True
    color: str | None = None

    @classmethod
    def from_api(cls, data: dict[str, Any]) -> "AppSummary":
        return cls(
            id=str(data.get("id", "")),
            title=str(data.get("title") or data.get("name") or "Untitled"),
            bundle_id=data.get("bundle_id") or data.get("bundleId"),
            icon_url=data.get("icon_url") or data.get("icon"),
            badge=data.get("badge"),
            version=data.get("version"),
            active=_as_bool(data.get("active", data.get("is_active", True))),
            in_store=_as_bool(data.get("in_store", data.get("available_in_store", True))),
            color=data.get("color"),
        )


@dataclass(frozen=True)
class AppVersion:
    version: str
    label: str
    title: str | None = None
    description: str | None = None
    icon_url: str | None = None
    bundle_id: str | None = None
    app_store_id: str | None = None
    min_ios: str | None = None
    is_latest: bool = False
    in_store: bool | None = None
    source: str | None = None

    @classmethod
    def from_api(cls, data: dict[str, Any]) -> "AppVersion":
        version = str(data.get("version") or data.get("name") or data.get("label") or "unknown")
        return cls(
            version=version,
            label=str(data.get("label") or ("New version" if data.get("is_latest") else "Previous version")),
            title=data.get("title"),
            description=data.get("description"),
            icon_url=data.get("icon_url") or data.get("icon"),
            bundle_id=data.get("bundle_id") or data.get("bundleId"),
            app_store_id=str(data.get("app_store_id") or data.get("adam_id") or data.get("trackId") or "") or None,
            min_ios=data.get("min_ios") or data.get("minimum_os_version"),
            is_latest=_as_bool(data.get("is_latest", data.get("latest", False))),
            in_store=_optional_bool(data.get("in_store", data.get("available_in_store"))),
            source=data.get("source"),
        )


@dataclass(frozen=True)
class AppInfo:
    id: str
    title: str | None
    bundle_id: str | None
    app_store_id: str | None
    min_ios: str | None
    version: str | None = None
    icon_url: str | None = None
    versions: tuple[AppVersion, ...] = ()
    active: bool = True
    in_store: bool = True
    color: str | None = None

    @classmethod
    def from_api(cls, app_id: str, data: dict[str, Any]) -> "AppInfo":
        raw_versions = data.get("versions") if isinstance(data.get("versions"), list) else []
        versions = tuple(AppVersion.from_api(item) for item in raw_versions if isinstance(item, dict))
        return cls(
            id=str(data.get("id") or app_id),
            title=data.get("title") or data.get("name"),
            bundle_id=data.get("bundle_id") or data.get("bundleId"),
            app_store_id=str(data.get("app_store_id") or data.get("adam_id") or data.get("trackId") or "") or None,
            min_ios=data.get("min_ios") or data.get("minimum_os_version"),
            version=data.get("version"),
            icon_url=data.get("icon_url") or data.get("icon"),
            versions=versions,
            active=_as_bool(data.get("active", data.get("is_active", True))),
            in_store=_as_bool(data.get("in_store", data.get("available_in_store", True))),
            color=data.get("color"),
        )


@dataclass(frozen=True)
class DeviceInfo:
    udid: str
    product_type: str | None
    product_version: str | None
    device_name: str | None
    battery_current_capacity: int | None


@dataclass(frozen=True)
class InstalledApp:
    bundle_id: str
    name: str | None
    version: str | None


@dataclass(frozen=True)
class AuthSession:
    email: str
    created_at: datetime


@dataclass(frozen=True)
class OtaUpdate:
    has_update: bool
    version: str | None
    build: str | None
    download_url: str | None
    notes: str | None
    mandatory: bool = False

    @classmethod
    def from_api(cls, data: dict[str, Any]) -> "OtaUpdate":
        return cls(
            has_update=_as_bool(data.get("has_update", False)),
            version=str(data["version"]) if data.get("version") is not None else None,
            build=str(data["build"]) if data.get("build") is not None else None,
            download_url=str(data["download_url"]) if data.get("download_url") else None,
            notes=data.get("notes"),
            mandatory=_as_bool(data.get("mandatory", False)),
        )


def _as_bool(value: Any) -> bool:
    if isinstance(value, bool):
        return value
    if isinstance(value, int):
        return value != 0
    if isinstance(value, str):
        return value.strip().lower() in {"1", "true", "yes", "y", "on", "active", "available"}
    return False


def _optional_bool(value: Any) -> bool | None:
    if value is None:
        return None
    return _as_bool(value)
