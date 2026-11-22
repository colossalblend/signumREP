from __future__ import annotations

import json
import logging
import subprocess
import time
from pathlib import Path

from .api_lease import ApiLease
from .config import Config
from .production import configure_logs


def process_alive(cfg, service):
    pid_path = cfg.data_dir / "logs" / f"{service}.pid"
    try:
        identifier = int(pid_path.read_text().strip())
    except (OSError, ValueError):
        return False
    expected = (
        r"-m\s+signum_bot(?:\s|$)"
        if service == "bot"
        else r"-m\s+signum_bot\.api_server(?:\s|$)"
    )
    command = f"$entry=Get-CimInstance Win32_Process -Filter 'ProcessId={identifier}'; if($entry -and $entry.CommandLine -match '{expected}'){{'alive'}}"
    result = subprocess.run(
        ["powershell", "-NoProfile", "-Command", command],
        capture_output=True,
        timeout=10,
        creationflags=subprocess.CREATE_NO_WINDOW,
    )
    return result.returncode == 0 and b"alive" in result.stdout


def health_fresh(cfg, service):
    try:
        payload = json.loads(
            (cfg.data_dir / "logs" / f"{service}.health.json").read_text()
        )
        current = int((cfg.data_dir / "logs" / f"{service}.pid").read_text().strip())
        return payload["pid"] == current and time.time() - payload["updated_at"] < 120
    except (OSError, ValueError, KeyError):
        return False


def main():
    cfg = Config.load(require_bot_token=False)
    cfg.prepare()
    configure_logs(cfg, "supervisor")
    lease = ApiLease(
        cfg.data_dir / "supervisor.lock", "Контроль процессов уже запущен."
    )
    root = Path(__file__).resolve().parent.parent
    delays = {name: 5 for name in ("bot", "api")}
    next_check = {name: time.monotonic() + 120 for name in delays}
    try:
        while True:
            for service in delays:
                if time.monotonic() < next_check[service]:
                    continue
                try:
                    alive = process_alive(cfg, service)
                    if alive and health_fresh(cfg, service):
                        delays[service] = 5
                        continue
                    command = [
                        "powershell",
                        "-NoProfile",
                        "-ExecutionPolicy",
                        "Bypass",
                        "-File",
                        str(root / "tools" / f"start_{service}.ps1"),
                    ]
                    if alive:
                        command.append("-Restart")
                    subprocess.run(
                        command,
                        cwd=root,
                        timeout=30,
                        check=True,
                        capture_output=True,
                        creationflags=subprocess.CREATE_NO_WINDOW,
                    )
                    logging.info("Restarted %s", service)
                    next_check[service] = time.monotonic() + 120 + delays[service]
                    delays[service] = min(300, delays[service] * 2)
                except Exception as exc:
                    logging.error(
                        "Recovery failed for %s (%s)", service, type(exc).__name__
                    )
                    next_check[service] = time.monotonic() + delays[service]
                    delays[service] = min(300, delays[service] * 2)
            time.sleep(15)
    finally:
        lease.close()


if __name__ == "__main__":
    main()
