from __future__ import annotations

import plistlib
import re
from pathlib import Path

from signum.config import Settings
from signum.models import DeviceInfo, InstalledApp
from signum.utils import ProgressCallback, run_command, run_streaming_command


class DeviceManager:
    def __init__(self, settings: Settings) -> None:
        self.settings = settings

    async def get_device_info(self) -> DeviceInfo:
        try:
            output = await run_command([self.settings.ideviceinfo_path], timeout=8)
            info = self._parse_key_value_output(output)
            if info.get("UniqueDeviceID"):
                return DeviceInfo(
                    udid=info["UniqueDeviceID"],
                    product_type=info.get("ProductType"),
                    product_version=info.get("ProductVersion"),
                    device_name=info.get("DeviceName"),
                    battery_current_capacity=self._parse_int(info.get("BatteryCurrentCapacity")),
                )
        except Exception:
            pass

        await self.list_installed_apps()
        return DeviceInfo(
            udid="USB connected",
            product_type="iPhone",
            product_version=None,
            device_name="iPhone",
            battery_current_capacity=None,
        )

    async def list_installed_apps(self) -> dict[str, InstalledApp]:
        try:
            output = await run_command([self.settings.ideviceinstaller_path, "list", "--user", "--xml"], timeout=30)
            parsed = self._parse_installed_plist(output)
            if parsed:
                return parsed
        except Exception:
            pass

        output = await run_command([self.settings.ideviceinstaller_path, "list", "--user"], timeout=30)
        apps: dict[str, InstalledApp] = {}
        for line in output.splitlines():
            parsed = self._parse_installed_line(line)
            if parsed:
                apps[parsed.bundle_id] = parsed
        return apps

    async def install_ipa(self, ipa_path: Path, *, progress: ProgressCallback | None = None) -> None:
        if not ipa_path.exists():
            raise FileNotFoundError(f"IPA file not found: {ipa_path}")
        if progress:
            progress(5, "Starting install")
        await run_streaming_command(
            [self.settings.ideviceinstaller_path, "install", ipa_path],
            progress=progress,
            timeout=300,
        )
        if progress:
            progress(100, "Install complete")

    async def reverse_backup(self, bundle_id: str, output_dir: Path) -> Path:
        output_dir.mkdir(parents=True, exist_ok=True)
        await run_command([self.settings.ideviceinstaller_path, "archive", bundle_id, f"--copy={output_dir}"], timeout=300)
        ipa_files = sorted(output_dir.glob("*.ipa"), key=lambda path: path.stat().st_mtime, reverse=True)
        if not ipa_files:
            raise RuntimeError("ideviceinstaller finished without an IPA file")
        return ipa_files[0]

    @staticmethod
    def _parse_key_value_output(output: str) -> dict[str, str]:
        result: dict[str, str] = {}
        for line in output.splitlines():
            if ": " not in line:
                continue
            key, value = line.split(": ", 1)
            result[key.strip()] = value.strip()
        return result

    @staticmethod
    def _parse_installed_line(line: str) -> InstalledApp | None:
        text = line.strip()
        if not text or text.startswith("Total:"):
            return None
        bundle_match = re.search(r"([A-Za-z0-9][A-Za-z0-9.-]+\.[A-Za-z0-9.-]+)", text)
        if not bundle_match:
            return None
        bundle_id = bundle_match.group(1)
        version_match = re.search(r"(\d+(?:\.\d+){1,4}(?:[-+][A-Za-z0-9.]+)?)", text[bundle_match.end() :])
        version = version_match.group(1) if version_match else None
        name = text.replace(bundle_id, "").strip(" -,\t")
        if version:
            name = name.replace(version, "").strip(" -,\t")
        return InstalledApp(bundle_id=bundle_id, name=name or None, version=version)

    @staticmethod
    def _parse_installed_plist(output: str) -> dict[str, InstalledApp]:
        try:
            payload = plistlib.loads(output.encode("utf-8"))
        except Exception:
            return {}

        raw_apps = payload.get("Applications") if isinstance(payload, dict) else payload
        if not isinstance(raw_apps, list):
            return {}

        apps: dict[str, InstalledApp] = {}
        for item in raw_apps:
            if not isinstance(item, dict):
                continue
            bundle_id = item.get("CFBundleIdentifier") or item.get("bundleIdentifier")
            if not bundle_id:
                continue
            version = item.get("CFBundleShortVersionString") or item.get("CFBundleVersion") or item.get("version")
            name = item.get("CFBundleDisplayName") or item.get("CFBundleName") or item.get("Path")
            apps[str(bundle_id)] = InstalledApp(
                bundle_id=str(bundle_id),
                name=str(name) if name else None,
                version=str(version) if version else None,
            )
        return apps

    @staticmethod
    def _parse_int(value: str | None) -> int | None:
        if value is None:
            return None
        try:
            return int(value)
        except ValueError:
            return None
