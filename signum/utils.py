from __future__ import annotations

import asyncio
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Callable, Iterable

from packaging.version import InvalidVersion, Version


ProgressCallback = Callable[[float, str], None]
ANSI_RE = re.compile(r"(?:\x1B|▯)\[[0-?]*[ -/]*[@-~]")
SENSITIVE_FLAGS = {"-p", "--password", "--auth-code", "--code", "--keychain-passphrase"}


class CommandError(RuntimeError):
    def __init__(self, command: Iterable[str | Path], returncode: int, output: str) -> None:
        self.command = mask_command(command)
        self.returncode = returncode
        self.output = clean_command_output(output)
        message = self.output.strip()[:400] or "no output"
        super().__init__(f"{' '.join(self.command)} failed with code {returncode}: {message}")


def normalize_command(command: Iterable[str | Path]) -> list[str]:
    return [str(part) for part in command]


def mask_command(command: Iterable[str | Path]) -> list[str]:
    parts = normalize_command(command)
    masked: list[str] = []
    hide_next = False
    for part in parts:
        if hide_next:
            masked.append("********")
            hide_next = False
            continue
        masked.append(part)
        if part in SENSITIVE_FLAGS:
            hide_next = True
    return masked


def clean_command_output(output: str) -> str:
    cleaned = ANSI_RE.sub("", output)
    cleaned = cleaned.replace("\r", "")
    return cleaned.strip()


def _subprocess_kwargs(parts: list[str]) -> dict:
    kwargs: dict = {}
    if sys.platform.startswith("win"):
        kwargs["creationflags"] = subprocess.CREATE_NO_WINDOW
    executable = Path(parts[0])
    parent = executable.parent
    if parent != Path(".") and parent.exists():
        env = os.environ.copy()
        env["PATH"] = f"{parent}{os.pathsep}{env.get('PATH', '')}"
        kwargs["cwd"] = str(parent)
        kwargs["env"] = env
    return kwargs


async def run_command(command: Iterable[str | Path], *, timeout: float | None = None) -> str:
    parts = normalize_command(command)
    process = await asyncio.create_subprocess_exec(
        *parts,
        stdin=asyncio.subprocess.DEVNULL,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.STDOUT,
        **_subprocess_kwargs(parts),
    )
    try:
        stdout, _ = await asyncio.wait_for(process.communicate(), timeout=timeout)
    except asyncio.TimeoutError:
        process.kill()
        await process.wait()
        raise
    output = clean_command_output(stdout.decode("utf-8", errors="replace") if stdout else "")
    if process.returncode:
        raise CommandError(parts, int(process.returncode), output)
    return output


async def run_streaming_command(
    command: Iterable[str | Path],
    *,
    progress: ProgressCallback | None = None,
    timeout: float | None = None,
) -> str:
    parts = normalize_command(command)
    process = await asyncio.create_subprocess_exec(
        *parts,
        stdin=asyncio.subprocess.DEVNULL,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.STDOUT,
        **_subprocess_kwargs(parts),
    )
    output_lines: list[str] = []

    async def consume() -> None:
        assert process.stdout is not None
        while True:
            line_bytes = await process.stdout.readline()
            if not line_bytes:
                break
            line = clean_command_output(line_bytes.decode("utf-8", errors="replace").strip())
            output_lines.append(line)
            percent = extract_percent(line)
            if progress and percent is not None:
                progress(percent, line)

    try:
        await asyncio.wait_for(consume(), timeout=timeout)
        returncode = await asyncio.wait_for(process.wait(), timeout=5)
    except asyncio.TimeoutError:
        process.kill()
        await process.wait()
        raise

    output = "\n".join(output_lines)
    if returncode:
        raise CommandError(parts, int(returncode), output)
    return output


def extract_percent(line: str) -> float | None:
    # ipatool-cpp emits "PROGRESS:42" on non-TTY stdout instead of a percent bar
    match = re.search(r"PROGRESS:(\d{1,3})", line) or re.search(r"(\d{1,3}(?:[.,]\d+)?)\s*%", line)
    if not match:
        return None
    value = float(match.group(1).replace(",", "."))
    return max(0.0, min(100.0, value))


def compare_versions(left: str, right: str) -> int:
    try:
        left_v = Version(left)
        right_v = Version(right)
    except InvalidVersion:
        left_parts = tuple(int(part) for part in re.findall(r"\d+", left))
        right_parts = tuple(int(part) for part in re.findall(r"\d+", right))
        return (left_parts > right_parts) - (left_parts < right_parts)
    return (left_v > right_v) - (left_v < right_v)


def looks_like_version(value: str | None) -> bool:
    if not value:
        return False
    return bool(re.match(r"^\d+(?:\.\d+)*(?:[-+][A-Za-z0-9.]+)?$", value.strip()))
