from __future__ import annotations

import fcntl
import json
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT))
os.chdir(ROOT)
os.umask(0o077)
for name in tuple(os.environ):
    if name.startswith("HTTP_") or name in {"REQUEST_URI", "REQUEST_METHOD", "QUERY_STRING", "SCRIPT_FILENAME"}:
        del os.environ[name]


def matches(pid, module):
    try:
        args = Path(f"/proc/{int(pid)}/cmdline").read_bytes().split(b"\0")
        return any(args[i:i + 2] == [b"-m", module.encode()] for i in range(len(args) - 1))
    except (OSError, ValueError):
        return False


def main():
    from signum_bot.config import Config
    cfg = Config.load()
    cfg.prepare()
    with (cfg.data_dir / "hosting-watchdog.lock").open("a") as lease:
        try:
            fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return
        path = cfg.data_dir / "hosting-processes.json"
        try:
            state = json.loads(path.read_text())
        except (OSError, ValueError):
            state = {}
        stop = len(sys.argv) > 1 and sys.argv[1] == "stop"
        stopping = []
        for service, module in (("api", "signum_bot.api_server"), ("bot", "signum_bot")):
            if service == "api" and cfg.api_embedded and not stop:
                continue
            entry = state.get(service, {})
            pid = entry.get("pid", 0)
            alive = matches(pid, module)
            if stop:
                try:
                    health = json.loads((cfg.data_dir / "logs" / f"{service}.health.json").read_text())
                    if matches(health["pid"], module):
                        pid = health["pid"]
                        alive = True
                except (OSError, ValueError, KeyError):
                    pass
                if alive:
                    os.kill(pid, signal.SIGTERM)
                    stopping.append((pid, module))
                continue
            if (ROOT / "DISABLED").exists():
                continue
            fresh = False
            try:
                health = json.loads((cfg.data_dir / "logs" / f"{service}.health.json").read_text())
                fresh = time.time() - health["updated_at"] < 120
            except (OSError, ValueError, KeyError):
                pass
            if fresh or (alive and time.time() - entry.get("started", 0) < 120):
                continue
            if alive:
                os.kill(pid, signal.SIGTERM)
                continue
            process = subprocess.Popen(
                [str(ROOT / "bin/python"), "-m", module], cwd=ROOT,
                stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL, start_new_session=True,
            )
            state[service] = {"pid": process.pid, "started": time.time()}
        pending = path.with_suffix(".part")
        pending.write_text(json.dumps(state))
        os.replace(pending, path)
        for pid, module in stopping:
            try:
                subprocess.run(["/usr/bin/tail", f"--pid={pid}", "-f", "/dev/null"],
                               timeout=8, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            except subprocess.TimeoutExpired:
                if matches(pid, module):
                    os.kill(pid, signal.SIGKILL)


if __name__ == "__main__":
    main()
