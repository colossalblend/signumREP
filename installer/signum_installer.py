from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request
import winreg
import zipfile
from pathlib import Path

APP_NAME = "Signum"
APP_VERSION = "1.1.2"
WEBVIEW2_GUID = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
WEBVIEW2_BOOTSTRAP = "https://go.microsoft.com/fwlink/p/?LinkId=2124703"

                                     
BG = "#070912"
BG2 = "#0b1020"
SURFACE = "#151d34"
TEXT = "#f7f8ff"
MUTED = "#94a3b8"
BORDER = "#22283a"
BRAND = "#7c5cff"
BRAND2 = "#8b6dff"
BRAND_HOVER = "#9b87f5"
DANGER = "#f87171"
OK = "#6ee7b7"

WIN_W = 400
WIN_H = 468


def resource_path(name: str) -> Path:
    base = Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parent))
    return base / name


def default_target() -> Path:
    override = os.getenv("SIGNUM_INSTALL_TARGET")
    if override:
        return Path(override)
    return Path(os.environ["LOCALAPPDATA"]) / APP_NAME


def creationflags() -> int:
    return subprocess.CREATE_NO_WINDOW if sys.platform.startswith("win") else 0


def escape_ps(value: str) -> str:
    return value.replace("'", "''")


def icon_path() -> Path | None:
    for candidate in (
        resource_path("signum.ico"),
        Path(__file__).resolve().parents[1] / "assets" / "signum.ico",
    ):
        if candidate.is_file():
            return candidate
    return None


def stop_running_app() -> None:
    subprocess.run(
        ["taskkill", "/F", "/IM", "signum.exe"],
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        creationflags=creationflags(),
    )
    time.sleep(0.35)


def webview2_installed() -> bool:
    roots = (
        (winreg.HKEY_CURRENT_USER, rf"Software\Microsoft\EdgeUpdate\Clients\{WEBVIEW2_GUID}"),
        (winreg.HKEY_LOCAL_MACHINE, rf"Software\WOW6432Node\Microsoft\EdgeUpdate\Clients\{WEBVIEW2_GUID}"),
        (winreg.HKEY_LOCAL_MACHINE, rf"Software\Microsoft\EdgeUpdate\Clients\{WEBVIEW2_GUID}"),
    )
    for hive, key in roots:
        try:
            with winreg.OpenKey(hive, key) as handle:
                pv, _ = winreg.QueryValueEx(handle, "pv")
            if str(pv).strip() and str(pv) != "0.0.0.0":
                return True
        except OSError:
            continue
    return False


def ensure_webview2(progress) -> None:
    if webview2_installed():
        return
    progress(18, "Готовим компоненты Windows…")
    tmp = Path(tempfile.gettempdir()) / "MicrosoftEdgeWebview2Setup.exe"
    urllib.request.urlretrieve(WEBVIEW2_BOOTSTRAP, tmp)
    subprocess.run(
        [str(tmp), "/silent", "/install"],
        check=False,
        creationflags=creationflags(),
    )


def create_shortcut(folder_name: str, exe_path: Path, working_dir: Path) -> None:
    script = (
        "$shell=New-Object -ComObject WScript.Shell;"
        f"$shortcut=$shell.CreateShortcut([Environment]::GetFolderPath('{folder_name}') + '\\Signum.lnk');"
        f"$shortcut.TargetPath='{escape_ps(str(exe_path))}';"
        f"$shortcut.WorkingDirectory='{escape_ps(str(working_dir))}';"
        f"$shortcut.IconLocation='{escape_ps(str(exe_path))}';"
        "$shortcut.Save()"
    )
    subprocess.run(
        ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", script],
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        creationflags=creationflags(),
    )


def write_uninstaller(target: Path, exe_path: Path) -> Path:
    script = target / "uninstall.cmd"
    script.write_text(
        "@echo off\r\n"
        f'if exist "{exe_path}" (\r\n'
        f'  start "" "{exe_path}" --uninstall\r\n'
        "  exit /b 0\r\n"
        ")\r\n"
        "taskkill /F /IM signum.exe >nul 2>&1\r\n"
        f'del /f /q "%APPDATA%\\Microsoft\\Windows\\Start Menu\\Programs\\Signum.lnk" >nul 2>&1\r\n'
        f'del /f /q "%USERPROFILE%\\Desktop\\Signum.lnk" >nul 2>&1\r\n'
        "reg delete \"HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Signum\" /f >nul 2>&1\r\n"
        'rmdir /s /q "%USERPROFILE%\\.ipatool" >nul 2>&1\r\n'
        'rmdir /s /q "%APPDATA%\\pw.appbank.signum" >nul 2>&1\r\n'
        'rmdir /s /q "%LOCALAPPDATA%\\pw.appbank.signum" >nul 2>&1\r\n'
        f'rmdir /s /q "{target}\\history-secrets" >nul 2>&1\r\n'
        f'rmdir /s /q "{target}\\store-session" >nul 2>&1\r\n'
        f'rmdir /s /q "{target}\\history-debug" >nul 2>&1\r\n'
        f'rmdir /s /q "{target}\\resources" >nul 2>&1\r\n'
        f'del /f /q "{exe_path}" >nul 2>&1\r\n'
        f'del /f /q "{script}" >nul 2>&1\r\n'
        f'rmdir /s /q "{target}" >nul 2>&1\r\n',
        encoding="utf-8",
    )
    try:
        key = winreg.CreateKey(
            winreg.HKEY_CURRENT_USER,
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Signum",
        )
        winreg.SetValueEx(key, "DisplayName", 0, winreg.REG_SZ, APP_NAME)
        winreg.SetValueEx(key, "DisplayVersion", 0, winreg.REG_SZ, APP_VERSION)
        winreg.SetValueEx(key, "Publisher", 0, winreg.REG_SZ, APP_NAME)
        winreg.SetValueEx(key, "InstallLocation", 0, winreg.REG_SZ, str(target))
        winreg.SetValueEx(key, "DisplayIcon", 0, winreg.REG_SZ, str(exe_path))
        winreg.SetValueEx(key, "UninstallString", 0, winreg.REG_SZ, f'"{exe_path}" --uninstall')
        winreg.SetValueEx(key, "QuietUninstallString", 0, winreg.REG_SZ, f'"{exe_path}" --uninstall --silent')
        winreg.SetValueEx(key, "NoModify", 0, winreg.REG_DWORD, 1)
        winreg.SetValueEx(key, "NoRepair", 0, winreg.REG_DWORD, 1)
        winreg.CloseKey(key)
    except OSError:
        pass
    return script


def copy_with_retry(src: Path, dest: Path, attempts: int = 10) -> None:
    dest.parent.mkdir(parents=True, exist_ok=True)
    last: Exception | None = None
    for i in range(attempts):
        try:
            shutil.copy2(src, dest)
            return
        except Exception as exc:
            last = exc
            time.sleep(0.35 + i * 0.1)
    raise OSError(str(last) if last else "не удалось скопировать файл")


def install(
    target: Path,
    *,
    desktop_shortcut: bool,
    progress,
) -> Path:
    payload = resource_path("SignumPayload.zip")
    if not payload.exists():
        payload = Path(__file__).resolve().parent / "payload" / "SignumPayload.zip"
    if not payload.exists():
        raise FileNotFoundError("Не найден пакет установки. Соберите SignumSetup ещё раз.")

    progress(4, "Останавливаем предыдущую версию…")
    stop_running_app()
    ensure_webview2(progress)

    progress(22, "Распаковываем файлы…")
    target.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(payload) as archive:
        names = [n for n in archive.namelist() if not n.endswith("/")]
        total = max(len(names), 1)
        for index, name in enumerate(names, start=1):
            rel = Path(name)
            if rel.is_absolute() or ".." in rel.parts:
                continue
            dest = target / rel
            dest.parent.mkdir(parents=True, exist_ok=True)
            with archive.open(name) as src, dest.open("wb") as out:
                shutil.copyfileobj(src, out)
            pct = 22 + int(index / total * 62)
            progress(pct, f"Копируем файлы… {index} из {total}")

    exe_path = target / "signum.exe"
    if not exe_path.exists():
        raise FileNotFoundError("Не удалось установить Signum.")

    progress(88, "Ярлыки…")
    create_shortcut("Programs", exe_path, target)
    if desktop_shortcut:
        create_shortcut("Desktop", exe_path, target)
    write_uninstaller(target, exe_path)
    progress(100, "Готово")
    return exe_path


def launch_app(exe_path: Path) -> None:
    subprocess.Popen([str(exe_path)], cwd=str(exe_path.parent), creationflags=creationflags())


def lerp_hex(a: str, b: str, t: float) -> str:
    t = max(0.0, min(1.0, t))
    ar, ag, ab = int(a[1:3], 16), int(a[3:5], 16), int(a[5:7], 16)
    br, bg, bb = int(b[1:3], 16), int(b[3:5], 16), int(b[5:7], 16)
    return "#{:02x}{:02x}{:02x}".format(
        int(ar + (br - ar) * t),
        int(ag + (bg - ag) * t),
        int(ab + (bb - ab) * t),
    )


def enable_dpi() -> None:
    try:
        from ctypes import windll

        windll.shcore.SetProcessDpiAwareness(2)
    except Exception:
        try:
            from ctypes import windll

            windll.user32.SetProcessDPIAware()
        except Exception:
            pass


def round_window(root) -> None:
    try:
        from ctypes import byref, c_int, sizeof, windll

        root.update_idletasks()
        hwnd = windll.user32.GetParent(root.winfo_id())
        if not hwnd:
            hwnd = int(root.frame(), 16) if hasattr(root, "frame") else 0
        if hwnd:
            windll.dwmapi.DwmSetWindowAttribute(hwnd, 33, byref(c_int(2)), sizeof(c_int))
    except Exception:
        pass


class SetupApp:
    def __init__(self, target: Path, start_app: bool) -> None:
        import tkinter as tk

        self.tk = tk
        self.target = target
        self.start_app = start_app
        self.exe_path: Path | None = None
        self.desktop_shortcut = True
        self.busy = False
        self._bar = 0.0
        self._bar_to = 0.0
        self._hover = 0.0
        self._hover_to = 0.0
        self._pulse = 0.0
        self.page = "welcome"
        self._drag: tuple[int, int] | None = None

        root = tk.Tk()
        self.root = root
        root.title(APP_NAME)
        root.configure(bg=BG)
        root.resizable(False, False)
        try:
            root.attributes("-topmost", True)
            root.after(400, lambda: root.attributes("-topmost", False))
        except Exception:
            pass
        ico = icon_path()
        if ico:
            try:
                root.iconbitmap(str(ico))
            except Exception:
                pass
        sw, sh = root.winfo_screenwidth(), root.winfo_screenheight()
        root.geometry(f"{WIN_W}x{WIN_H}+{(sw - WIN_W) // 2}+{(sh - WIN_H) // 2}")
        root.minsize(WIN_W, WIN_H)
        root.protocol("WM_DELETE_WINDOW", self.on_close)
        root.bind("<Escape>", lambda _e: self.on_close())

        self.canvas = tk.Canvas(
            root, width=WIN_W, height=WIN_H, bg=BG, highlightthickness=0, bd=0
        )
        self.canvas.pack(fill="both", expand=True)
        self.canvas.bind("<ButtonPress-1>", self._press)
        self.canvas.bind("<B1-Motion>", self._move)
        self.canvas.bind("<ButtonRelease-1>", self._release)
        self.canvas.bind("<Motion>", self._motion)
        self.canvas.bind("<Leave>", lambda _e: self._set_hover(False))

        self.photo = None
        png = Path(__file__).resolve().parents[1] / "assets" / "signum-icon.png"
        if not png.is_file():
            png = resource_path("signum-icon.png")
        if png.is_file():
            try:
                from PIL import Image, ImageTk

                img = Image.open(png).convert("RGBA").resize((72, 72), Image.Resampling.LANCZOS)
                self.photo = ImageTk.PhotoImage(img)
            except Exception:
                try:
                    self.photo = tk.PhotoImage(file=str(png))
                except Exception:
                    self.photo = None

        self.draw()
        self._tick()
        round_window(root)

    def _press(self, event) -> None:
        if event.y <= 44 and event.x < WIN_W - 48:
            self._drag = (event.x_root, event.y_root)
        self._hit(event, press=True)

    def _move(self, event) -> None:
        if not self._drag:
            return
        dx = event.x_root - self._drag[0]
        dy = event.y_root - self._drag[1]
        self._drag = (event.x_root, event.y_root)
        x = self.root.winfo_x() + dx
        y = self.root.winfo_y() + dy
        self.root.geometry(f"+{x}+{y}")

    def _release(self, event) -> None:
        self._drag = None
        self._hit(event, press=False, click=True)

    def _motion(self, event) -> None:
        self._set_hover(self._in_primary(event.x, event.y))
        if self._in_close(event.x, event.y) or self._in_primary(event.x, event.y) or self._in_check(event.x, event.y) or self._in_secondary(event.x, event.y):
            self.canvas.configure(cursor="hand2")
        else:
            self.canvas.configure(cursor="arrow")

    def _set_hover(self, on: bool) -> None:
        self._hover_to = 1.0 if on else 0.0

    def _in_close(self, x: int, y: int) -> bool:
        return WIN_W - 42 <= x <= WIN_W - 14 and 12 <= y <= 40

    def _in_check(self, x: int, y: int) -> bool:
        return self.page == "welcome" and 28 <= x <= 360 and 292 <= y <= 328

    def _primary_box(self) -> tuple[int, int, int, int]:
        if self.page == "install":
            return (0, 0, 0, 0)
        if self.page == "done":
            return (28, 348, 372, 396)
        return (28, 360, 372, 408)

    def _secondary_box(self) -> tuple[int, int, int, int]:
        if self.page == "done":
            return (28, 412, 372, 444)
        return (0, 0, 0, 0)

    def _in_primary(self, x: int, y: int) -> bool:
        x0, y0, x1, y1 = self._primary_box()
        return x0 < x1 and x0 <= x <= x1 and y0 <= y <= y1

    def _in_secondary(self, x: int, y: int) -> bool:
        x0, y0, x1, y1 = self._secondary_box()
        return x0 < x1 and x0 <= x <= x1 and y0 <= y <= y1

    def _hit(self, event, *, press: bool, click: bool = False) -> None:
        if click and self._in_close(event.x, event.y):
            self.on_close()
            return
        if click and self._in_check(event.x, event.y) and not self.busy:
            self.desktop_shortcut = not self.desktop_shortcut
            self.draw()
            return
        if click and self._in_primary(event.x, event.y):
            if self.page == "welcome":
                self.begin()
            elif self.page == "done":
                if self.exe_path:
                    launch_app(self.exe_path)
                self.root.destroy()
            elif self.page == "error":
                self.root.destroy()
            return
        if click and self._in_secondary(event.x, event.y) and self.page == "done":
            self.root.destroy()

    def on_close(self) -> None:
        if self.busy:
            return
        self.root.destroy()

    def begin(self) -> None:
        if self.busy:
            return
        self.busy = True
        self.page = "install"
        self._bar = 0.0
        self._bar_to = 2.0
        self.draw()

        def work() -> None:
            try:
                path = install(
                    self.target,
                    desktop_shortcut=self.desktop_shortcut,
                    progress=self._progress,
                )
            except Exception as exc:
                self.root.after(0, lambda: self.fail(str(exc)))
                return
            self.exe_path = path
            self.root.after(0, self.finish)

        threading.Thread(target=work, daemon=True).start()

    def _progress(self, pct: int, message: str) -> None:
        self.root.after(0, lambda: self._apply_progress(pct, message))

    def _apply_progress(self, pct: int, message: str) -> None:
        self._bar_to = float(max(0, min(100, pct)))
        self.status = message
        self.draw()

    def finish(self) -> None:
        self.busy = False
        self.page = "done"
        self._bar_to = 100.0
        self.draw()
        if self.start_app and self.exe_path:
                                                                    
            pass

    def fail(self, message: str) -> None:
        self.busy = False
        self.page = "error"
        self.error = message
        self.draw()

    def _tick(self) -> None:
        changed = False
        if abs(self._bar - self._bar_to) > 0.15:
            self._bar += (self._bar_to - self._bar) * 0.16
            changed = True
        elif self._bar != self._bar_to:
            self._bar = self._bar_to
            changed = True
        if abs(self._hover - self._hover_to) > 0.04:
            self._hover += (self._hover_to - self._hover) * 0.28
            changed = True
        self._pulse = (self._pulse + 0.045) % 6.2832
        if self.page == "welcome" or changed or self.page == "install":
            self.draw()
        self.root.after(16, self._tick)

    def _round_rect(self, x0, y0, x1, y1, r, **kw):
        r = min(r, (x1 - x0) / 2, (y1 - y0) / 2)
        pts = [
            x0 + r, y0, x1 - r, y0, x1, y0, x1, y0 + r,
            x1, y1 - r, x1, y1, x1 - r, y1, x0 + r, y1,
            x0, y1, x0, y1 - r, x0, y0 + r, x0, y0,
        ]
        return self.canvas.create_polygon(pts, smooth=True, splinesteps=12, **kw)

    def draw(self) -> None:
        c = self.canvas
        c.delete("all")
        c.create_rectangle(0, 0, WIN_W, WIN_H, fill=BG, outline="")
                                                              
        glow = 0.10 + 0.04 * (0.5 + 0.5 * __import__("math").sin(self._pulse))
        c.create_oval(-80, -120, 220, 180, fill=BG2, outline="")
        c.create_oval(240, 300, 520, 560, fill=BG2, outline="")

               
        close_col = DANGER if False else MUTED
        c.create_text(WIN_W - 28, 26, text="✕", fill=close_col, font=("Segoe UI", 11))

        c.create_text(20, 26, text="Установка", fill=MUTED, font=("Segoe UI Semibold", 10), anchor="w")

        if self.page == "welcome":
            self._draw_welcome(glow)
        elif self.page == "install":
            self._draw_install()
        elif self.page == "done":
            self._draw_done()
        else:
            self._draw_error()

    def _draw_icon(self, y: int) -> None:
        cx = WIN_W // 2
        self._round_rect(cx - 44, y, cx + 44, y + 88, 22, fill=SURFACE, outline=BORDER)
        if self.photo:
            self.canvas.create_image(cx, y + 44, image=self.photo)
        else:
            self.canvas.create_text(cx, y + 44, text="S", fill=TEXT, font=("Segoe UI Semibold", 28))

    def _draw_welcome(self, glow: float) -> None:
        self._draw_icon(72)
        self.canvas.create_text(WIN_W // 2, 184, text="Signum", fill=TEXT, font=("Segoe UI Semibold", 22))
        self.canvas.create_text(
            WIN_W // 2, 214, text=f"Версия {APP_VERSION}", fill=MUTED, font=("Segoe UI", 10)
        )
        self.canvas.create_text(
            WIN_W // 2,
            250,
            text="Установка на этот компьютер",
            fill=TEXT,
            font=("Segoe UI", 11),
        )
        self.canvas.create_text(
            WIN_W // 2,
            272,
            text=str(self.target),
            fill=MUTED,
            font=("Segoe UI", 8),
            width=340,
        )
        box = SURFACE
        self._round_rect(28, 296, 372, 332, 12, fill=box, outline=BORDER)
        mark = BRAND if self.desktop_shortcut else BORDER
        self._round_rect(40, 306, 60, 326, 6, fill=mark, outline="")
        if self.desktop_shortcut:
            self.canvas.create_text(50, 316, text="✓", fill=TEXT, font=("Segoe UI Semibold", 10))
        self.canvas.create_text(
            72, 314, text="Ярлык на рабочем столе", fill=TEXT, font=("Segoe UI", 10), anchor="w"
        )
        self._draw_primary(28, 360, 372, 408, "Установить")

    def _draw_install(self) -> None:
        self._draw_icon(88)
        self.canvas.create_text(WIN_W // 2, 204, text="Устанавливаем Signum", fill=TEXT, font=("Segoe UI Semibold", 16))
        status = getattr(self, "status", "Подготовка…")
        self.canvas.create_text(WIN_W // 2, 232, text=status, fill=MUTED, font=("Segoe UI", 10), width=340)
        x0, y0, x1, y1 = 40, 300, 360, 312
        self._round_rect(x0, y0, x1, y1, 6, fill=SURFACE, outline="")
        width = x1 - x0
        fill_w = max(12, int(width * (self._bar / 100.0)))
        self._round_rect(x0, y0, x0 + fill_w, y1, 6, fill=BRAND, outline="")
        self.canvas.create_text(
            WIN_W // 2, 336, text=f"{int(self._bar)}%", fill=TEXT, font=("Segoe UI Semibold", 12)
        )

    def _draw_done(self) -> None:
        self._draw_icon(80)
        self.canvas.create_text(WIN_W // 2, 196, text="Готово", fill=TEXT, font=("Segoe UI Semibold", 20))
        self.canvas.create_text(
            WIN_W // 2,
            228,
            text="Signum установлен и готов к работе.",
            fill=MUTED,
            font=("Segoe UI", 10),
        )
        self._draw_primary(28, 348, 372, 396, "Запустить")
        self._round_rect(28, 412, 372, 444, 12, fill=SURFACE, outline=BORDER)
        self.canvas.create_text(WIN_W // 2, 428, text="Закрыть", fill=TEXT, font=("Segoe UI Semibold", 10))

    def _draw_error(self) -> None:
        self._draw_icon(80)
        self.canvas.create_text(WIN_W // 2, 196, text="Не удалось установить", fill=TEXT, font=("Segoe UI Semibold", 16))
        self.canvas.create_text(
            WIN_W // 2,
            236,
            text=getattr(self, "error", "Повторите попытку."),
            fill=DANGER,
            font=("Segoe UI", 10),
            width=320,
        )
        self._draw_primary(28, 360, 372, 408, "Закрыть")

    def _draw_primary(self, x0, y0, x1, y1, label: str) -> None:
        fill = lerp_hex(BRAND, BRAND_HOVER, self._hover)
        self._round_rect(x0, y0, x1, y1, 14, fill=fill, outline="")
        self.canvas.create_text((x0 + x1) // 2, (y0 + y1) // 2, text=label, fill=TEXT, font=("Segoe UI Semibold", 12))

    def run(self) -> int:
        self.root.mainloop()
        return 0


def run_gui(target: Path, *, start_app: bool) -> int:
    enable_dpi()
    return SetupApp(target, start_app).run()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--silent", action="store_true")
    parser.add_argument("--no-start", action="store_true")
    parser.add_argument("--no-desktop", action="store_true")
    args = parser.parse_args()
    target = default_target()
    if args.silent:
        def _p(_pct: int, _msg: str) -> None:
            return None

        exe = install(target, desktop_shortcut=not args.no_desktop, progress=_p)
        if not args.no_start:
            launch_app(exe)
        return 0
    return run_gui(target, start_app=not args.no_start)


if __name__ == "__main__":
    raise SystemExit(main())
