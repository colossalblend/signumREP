from __future__ import annotations

import asyncio
import os
from pathlib import Path
from typing import Callable

import flet as ft

from signum.api import ApiClient
from signum.config import RESOURCE_DIR, Settings
from signum.device import DeviceManager
from signum.ipatool import IpaToolManager
from signum.models import AppInfo, AppSummary, AppVersion, DeviceInfo, InstalledApp, OtaUpdate
from signum.ota import OtaManager
from signum.storage import CredentialStore
from signum.updater import SmartAutoUpdater, UpdateCandidate
from signum.utils import compare_versions


ACCENT = "#7C5CFF"
ACCENT_2 = "#24C8DB"
BG = "#090B12"
PANEL = "#161A26"
TEXT_MUTED = "#9DA7BD"
ERROR = "#FF596D"
SUCCESS = "#3FE28F"
WARNING = "#FFB86B"


def border_all(width: float, color: str) -> ft.Border:
    side = ft.BorderSide(width=width, color=color)
    return ft.Border(top=side, right=side, bottom=side, left=side)


def border_only(
    *,
    top: ft.BorderSide | None = None,
    right: ft.BorderSide | None = None,
    bottom: ft.BorderSide | None = None,
    left: ft.BorderSide | None = None,
) -> ft.Border:
    return ft.Border(top=top, right=right, bottom=bottom, left=left)


def glass_container(
    content: ft.Control,
    *,
    padding: int | ft.Padding = 18,
    width: float | None = None,
    height: float | None = None,
    expand: bool | int | None = None,
    border_radius: int = 8,
) -> ft.Container:
    return ft.Container(
        content=content,
        width=width,
        height=height,
        expand=expand,
        padding=padding,
        bgcolor="#33FFFFFF",
        border=border_all(1, "#24FFFFFF"),
        border_radius=border_radius,
        blur=ft.Blur(16, 16),
        shadow=ft.BoxShadow(blur_radius=24, color="#22000000", offset=ft.Offset(0, 14)),
        animate_opacity=ft.Animation(220, ft.AnimationCurve.EASE_OUT),
        animate_scale=ft.Animation(180, ft.AnimationCurve.EASE_OUT),
    )


def icon_button(icon: str, tooltip: str, on_click: Callable[[ft.ControlEvent], object]) -> ft.IconButton:
    return ft.IconButton(
        icon=icon,
        tooltip=tooltip,
        icon_color="#DDE6FF",
        bgcolor="#14FFFFFF",
        hover_color="#22FFFFFF",
        width=38,
        height=38,
        style=ft.ButtonStyle(shape=ft.RoundedRectangleBorder(radius=8)),
        on_click=on_click,
    )


def plain_action_button(label: str, icon: str, on_click: Callable[[ft.ControlEvent], object]) -> ft.Control:
    return ft.Container(
        padding=ft.Padding(left=12, right=14, top=9, bottom=9),
        border_radius=8,
        bgcolor="#18FFFFFF",
        ink=True,
        on_click=on_click,
        content=ft.Row(
            tight=True,
            spacing=7,
            controls=[
                ft.Icon(icon, size=16, color="#DDE6FF"),
                ft.Text(label, size=13, color="#F4F7FF", weight=ft.FontWeight.W_600),
            ],
        ),
    )


def plain_icon_button(icon: str, tooltip: str, on_click: Callable[[ft.ControlEvent], object]) -> ft.Control:
    return ft.Container(
        width=38,
        height=38,
        border_radius=8,
        bgcolor="#14FFFFFF",
        ink=True,
        tooltip=tooltip,
        alignment=ft.Alignment(0, 0),
        on_click=on_click,
        content=ft.Icon(icon, size=20, color="#DDE6FF"),
    )


def set_hidden_titlebar(page: ft.Page) -> None:
    page.title = "Signum"
    try:
        page.window.title = "Signum"
        page.window.title_bar_hidden = True
        page.window.title_bar_buttons_hidden = True
        page.window.frameless = True
    except Exception:
        page.window_title_bar_hidden = True
        page.window_title_bar_buttons_hidden = True
        page.window_frameless = True


def minimize_window(page: ft.Page) -> None:
    try:
        page.window.minimized = True
        page.update()
    except Exception:
        pass


def close_window(page: ft.Page) -> None:
    os._exit(0)


def toggle_maximize_window(page: ft.Page) -> None:
    try:
        page.window.maximized = not bool(page.window.maximized)
        page.update()
    except Exception:
        pass


def show_snack(page: ft.Page, message: str, *, color: str = ACCENT) -> None:
    try:
        page.snack_bar = ft.SnackBar(
            ft.Text(message, color="#FFFFFF"),
            bgcolor=color,
            behavior=ft.SnackBarBehavior.FLOATING,
            margin=20,
            shape=ft.RoundedRectangleBorder(radius=8),
        )
        page.snack_bar.open = True
        page.update()
    except RuntimeError:
        return


class SignumApp:
    def __init__(self, page: ft.Page) -> None:
        self.page = page
        self.settings = Settings.load()
        self.store = CredentialStore()
        self.api = ApiClient(
            self.settings.api_base_url,
            app_version=self.settings.app_version,
            app_build=self.settings.app_build,
            devkey=self.settings.devkey,
            timeout=self.settings.api_timeout,
        )
        self.device = DeviceManager(self.settings)
        self.ipatool = IpaToolManager(self.settings, self.store)
        self.ota = OtaManager(self.settings)
        self.updater = SmartAutoUpdater(
            api=self.api,
            device=self.device,
            ipatool=self.ipatool,
            store=self.store,
            on_update_found=self.on_update_found,
            interval_seconds=self.settings.update_interval_seconds,
        )

        self.apps: list[AppSummary] = []
        self.app_infos: dict[str, AppInfo] = {}
        self.device_info: DeviceInfo | None = None
        self.installed_apps: dict[str, InstalledApp] = {}
        self.selected_index = 0
        self.current_dialog: ft.AlertDialog | None = None
        self.closed = False

        self.content = ft.Container(expand=True)
        self.grid = ft.GridView(expand=True, runs_count=3, max_extent=260, child_aspect_ratio=0.88, spacing=14, run_spacing=14)
        self.progress = ft.ProgressBar(value=0, color=ACCENT_2, bgcolor="#22FFFFFF", visible=False, width=220)
        self.progress_text = ft.Text("", size=13, color=TEXT_MUTED)
        self.file_picker = ft.FilePicker()
        self.page.services.append(self.file_picker)

    async def start(self) -> None:
        self.configure_page()
        self.page.add(self.build_shell())
        self.render_home_loading()
        self.page.update()
        asyncio.create_task(self.refresh_apps())
        asyncio.create_task(self.refresh_device())
        asyncio.create_task(self.check_ota_update(silent=True, auto_install=True))
        await self.updater.start()

    def configure_page(self) -> None:
        set_hidden_titlebar(self.page)
        self.page.theme_mode = ft.ThemeMode.DARK
        self.page.bgcolor = BG
        self.page.padding = 0
        self.page.spacing = 0
        try:
            self.page.window.width = 1180
            self.page.window.height = 760
            self.page.window.min_width = 980
            self.page.window.min_height = 640
            self.page.window.maximizable = True
            self.page.window.resizable = True
        except Exception:
            pass
        self.page.on_disconnect = self.on_disconnect

    def on_disconnect(self, _: ft.ControlEvent) -> None:
        self.closed = True
        asyncio.create_task(self.updater.stop())

    def request_close(self) -> None:
        self.closed = True
        try:
            if self.current_dialog:
                self.current_dialog.open = False
            if self.page.banner:
                self.page.banner.open = False
            self.progress.visible = False
            self.progress_text.value = ""
        except Exception:
            pass
        close_window(self.page)

    def safe_update(self) -> None:
        if self.closed:
            return
        try:
            self.page.update()
        except RuntimeError:
            self.closed = True

    def build_shell(self) -> ft.Control:
        return ft.Stack(
            expand=True,
            controls=[
                ft.Container(
                    expand=True,
                    gradient=ft.LinearGradient(
                        begin=ft.Alignment(-1, -1),
                        end=ft.Alignment(1, 1),
                        colors=["#090B12", "#111424", "#0A1021"],
                    ),
                ),
                ft.Column(
                    expand=True,
                    spacing=0,
                    controls=[
                        self.build_titlebar(),
                        ft.Row(
                            expand=True,
                            spacing=0,
                            controls=[
                                self.build_rail(),
                                ft.Container(
                                    expand=True,
                                    padding=ft.Padding(left=24, right=28, top=18, bottom=24),
                                    content=self.content,
                                ),
                            ],
                        ),
                    ],
                ),
            ],
        )

    def build_titlebar(self) -> ft.Control:
        drag_area = ft.WindowDragArea(
            expand=True,
            content=ft.Row(
                controls=[
                    ft.Container(
                        width=36,
                        height=36,
                        border_radius=8,
                        bgcolor="#1E2640",
                        alignment=ft.Alignment(0, 0),
                        content=ft.Image(src="assets/signum-icon.png", width=30, height=30, fit=ft.BoxFit.CONTAIN),
                    ),
                    ft.Column(
                        spacing=0,
                        alignment=ft.MainAxisAlignment.CENTER,
                        controls=[
                            ft.Text("Signum", size=16, weight=ft.FontWeight.W_700, color="#F4F7FF"),
                            ft.Text(f"Beta {self.settings.app_version} build {self.settings.app_build}", size=11, color=TEXT_MUTED),
                        ],
                    ),
                ],
            ),
        )
        return ft.Container(
            height=64,
            padding=ft.Padding(left=18, right=14, top=0, bottom=0),
            bgcolor="#660B0E18",
            border=border_only(bottom=ft.BorderSide(width=1, color="#18FFFFFF")),
            content=ft.Row(
                controls=[
                    drag_area,
                    self.progress,
                    ft.Container(width=8),
                    icon_button(ft.Icons.REMOVE, "Свернуть", lambda _: minimize_window(self.page)),
                    icon_button(ft.Icons.CROP_SQUARE_ROUNDED, "Развернуть", lambda _: toggle_maximize_window(self.page)),
                    plain_icon_button(ft.Icons.CLOSE, "Закрыть", lambda _: self.request_close()),
                ],
            ),
        )

    def build_rail(self) -> ft.Control:
        def on_change(e: ft.ControlEvent) -> None:
            self.selected_index = int(e.control.selected_index)
            self.render_current_view()

        return ft.Container(
            width=92,
            border=border_only(right=ft.BorderSide(width=1, color="#18FFFFFF")),
            content=ft.NavigationRail(
                selected_index=0,
                min_width=82,
                group_alignment=-0.86,
                bgcolor="#440F1320",
                indicator_color="#227C5CFF",
                label_type=ft.NavigationRailLabelType.NONE,
                on_change=on_change,
                destinations=[
                    ft.NavigationRailDestination(icon=ft.Icons.APPS_ROUNDED, selected_icon=ft.Icons.APPS, label="Главная"),
                    ft.NavigationRailDestination(icon=ft.Icons.PHONE_IPHONE_ROUNDED, selected_icon=ft.Icons.PHONE_IPHONE, label="Устройство"),
                    ft.NavigationRailDestination(icon=ft.Icons.PERSON_OUTLINE, selected_icon=ft.Icons.PERSON, label="Аккаунт"),
                    ft.NavigationRailDestination(icon=ft.Icons.SEND_OUTLINED, selected_icon=ft.Icons.SEND, label="AppBank"),
                ],
            ),
        )

    def render_current_view(self) -> None:
        if self.selected_index == 0:
            self.render_home()
        elif self.selected_index == 1:
            self.render_device()
        elif self.selected_index == 2:
            self.render_account()
        else:
            self.render_telegram()
        self.page.update()

    def view_header(self, title: str, subtitle: str, actions: list[ft.Control] | None = None) -> ft.Control:
        return ft.Row(
            alignment=ft.MainAxisAlignment.SPACE_BETWEEN,
            controls=[
                ft.Column(
                    spacing=2,
                    controls=[
                        ft.Text(title, size=30, weight=ft.FontWeight.W_700, color="#F5F7FF"),
                        ft.Text(subtitle, size=13, color=TEXT_MUTED),
                    ],
                ),
                ft.Row(spacing=8, controls=actions or []),
            ],
        )

    def beta_notice(self) -> ft.Control:
        return glass_container(
            ft.Row(
                spacing=12,
                controls=[
                    ft.Icon(ft.Icons.WARNING_AMBER_ROUNDED, color=WARNING),
                    ft.Column(
                        spacing=1,
                        controls=[
                            ft.Text("Бета-тест", size=14, weight=ft.FontWeight.W_700, color="#FFFFFF"),
                            ft.Text("Приложение находится в бета-тестировании и может работать нестабильно.", size=12, color=TEXT_MUTED),
                        ],
                    ),
                ],
            ),
            padding=14,
        )

    def render_home_loading(self) -> None:
        self.content.content = ft.Column(
            expand=True,
            spacing=18,
            controls=[
                self.view_header("Каталог", "Загрузка доступных банковских приложений"),
                self.beta_notice(),
                glass_container(
                    ft.Column(
                        [ft.ProgressRing(color=ACCENT_2), ft.Text("Подключаемся к AppBank API", color=TEXT_MUTED)],
                        horizontal_alignment=ft.CrossAxisAlignment.CENTER,
                        spacing=16,
                    ),
                    expand=True,
                ),
            ],
        )

    def render_home(self) -> None:
        self.grid.controls.clear()
        for app in self.apps:
            self.grid.controls.append(self.build_app_card(app))

        body: ft.Control
        if not self.apps:
            body = glass_container(
                ft.Column(
                    horizontal_alignment=ft.CrossAxisAlignment.CENTER,
                    alignment=ft.MainAxisAlignment.CENTER,
                    spacing=12,
                    controls=[
                        ft.Icon(ft.Icons.CLOUD_OFF_ROUNDED, size=44, color=TEXT_MUTED),
                        ft.Text("Каталог пока пуст", size=18, weight=ft.FontWeight.W_600),
                        ft.Text("Сейчас нет доступных приложений. Каталог обновится автоматически.", size=13, color=TEXT_MUTED),
                    ],
                ),
                expand=True,
            )
        else:
            body = self.grid

        self.content.content = ft.Column(
            expand=True,
            spacing=18,
            controls=[
                self.view_header(
                    "Каталог",
                    "Банки и важные приложения",
                    [
                        icon_button(ft.Icons.REFRESH_ROUNDED, "Обновить каталог", lambda _: asyncio.create_task(self.refresh_apps())),
                        icon_button(ft.Icons.UPLOAD_FILE_ROUNDED, "Установить IPA", lambda _: asyncio.create_task(self.open_ipa_picker())),
                    ],
                ),
                self.beta_notice(),
                self.build_drop_zone(),
                body,
            ],
        )

    def build_drop_zone(self) -> ft.Control:
        return glass_container(
            ft.Row(
                alignment=ft.MainAxisAlignment.SPACE_BETWEEN,
                controls=[
                    ft.Row(
                        spacing=12,
                        controls=[
                            ft.Icon(ft.Icons.FILE_UPLOAD_OUTLINED, color=ACCENT_2),
                            ft.Column(
                                spacing=1,
                                controls=[
                                    ft.Text("Быстрая установка .ipa", size=14, weight=ft.FontWeight.W_600),
                                    ft.Text("Выберите локальный IPA-файл для установки на подключенный iPhone.", size=12, color=TEXT_MUTED),
                                ],
                            ),
                        ],
                    ),
                    ft.FilledButton("Выбрать", icon=ft.Icons.FOLDER_OPEN_ROUNDED, on_click=lambda _: asyncio.create_task(self.open_ipa_picker()), style=ft.ButtonStyle(shape=ft.RoundedRectangleBorder(radius=8))),
                ],
            ),
            padding=14,
        )

    def build_app_card(self, app: AppSummary) -> ft.Control:
        image = ft.Image(src=app.icon_url, width=78, height=78, fit=ft.BoxFit.COVER, border_radius=18) if app.icon_url else ft.Icon(ft.Icons.ACCOUNT_BALANCE_ROUNDED, size=58, color=ACCENT_2)
        badge = ft.Container(
            visible=bool(app.badge),
            padding=ft.Padding(left=8, right=8, top=4, bottom=4),
            border_radius=8,
            bgcolor="#227C5CFF",
            content=ft.Text(app.badge or "", size=11, color="#FFFFFF", weight=ft.FontWeight.W_600),
        )
        card = glass_container(
            ft.Column(
                spacing=12,
                controls=[
                    ft.Row(alignment=ft.MainAxisAlignment.SPACE_BETWEEN, vertical_alignment=ft.CrossAxisAlignment.START, controls=[image, badge]),
                    ft.Container(height=5),
                    ft.Text(app.title, size=18, weight=ft.FontWeight.W_700, color="#F5F7FF", max_lines=2, overflow=ft.TextOverflow.ELLIPSIS),
                    ft.Text(f"Версия {app.version}" if app.version else "Версия уточняется", size=12, color=TEXT_MUTED),
                    ft.Text(self.availability_text(app), size=12, color=self.availability_color(app)),
                    ft.Container(expand=True),
                    ft.Row(
                        alignment=ft.MainAxisAlignment.SPACE_BETWEEN,
                        controls=[
                            ft.Text("#" + app.id, size=11, color="#68728A"),
                            ft.Icon(ft.Icons.ARROW_FORWARD_ROUNDED, size=18, color=ACCENT_2),
                        ],
                    ),
                ],
            ),
            padding=16,
            border_radius=8,
        )
        card.on_click = lambda _: asyncio.create_task(self.open_app_detail(app))
        card.on_hover = lambda e: self.hover_scale(e, 1.025)
        return card

    def hover_scale(self, e: ft.HoverEvent, scale: float) -> None:
        e.control.scale = scale if e.data == "true" else 1
        e.control.opacity = 0.92 if e.data == "true" else 1
        e.control.update()

    def version_in_store(self, info: AppInfo, version: AppVersion | None = None) -> bool:
        if version and version.in_store is not None:
            return version.in_store
        return info.in_store

    def availability_text(self, app: AppSummary | AppInfo, version: AppVersion | None = None) -> str:
        in_store = version.in_store if version and version.in_store is not None else app.in_store
        if in_store:
            return "В App Store"
        return "Удалено, доступно для восстановления"

    def availability_color(self, app: AppSummary | AppInfo, version: AppVersion | None = None) -> str:
        in_store = version.in_store if version and version.in_store is not None else app.in_store
        if in_store:
            return SUCCESS
        return WARNING

    def bundle_id_for(self, app: AppSummary | AppInfo, version: AppVersion | None = None) -> str | None:
        if isinstance(app, AppInfo):
            return version.bundle_id if version and version.bundle_id else app.bundle_id
        cached_info = self.app_infos.get(app.id)
        return app.bundle_id or (cached_info.bundle_id if cached_info else None)

    def installed_app_for(self, info: AppInfo, version: AppVersion | None = None) -> InstalledApp | None:
        bundle_id = self.bundle_id_for(info, version)
        return self.installed_apps.get(bundle_id) if bundle_id else None

    def installed_catalog_apps(self) -> list[tuple[AppSummary, str, InstalledApp]]:
        rows: list[tuple[AppSummary, str, InstalledApp]] = []
        for app in self.apps:
            bundle_id = self.bundle_id_for(app)
            if not bundle_id:
                continue
            installed = self.installed_apps.get(bundle_id)
            if installed:
                rows.append((app, bundle_id, installed))
        return rows

    def installed_status_text(self, info: AppInfo, version: AppVersion | None = None) -> str:
        bundle_id = self.bundle_id_for(info, version)
        if not bundle_id:
            return "bundle_id не указан"
        installed = self.installed_apps.get(bundle_id)
        if not installed:
            return f"На iPhone не найдено: {bundle_id}"
        version_text = f" {installed.version}" if installed.version else ""
        return f"На iPhone: {(installed.name or info.title or bundle_id)}{version_text}"

    def render_device(self) -> None:
        info = self.device_info
        if info is None:
            body = glass_container(
                ft.Column(
                    alignment=ft.MainAxisAlignment.CENTER,
                    horizontal_alignment=ft.CrossAxisAlignment.CENTER,
                    spacing=12,
                    controls=[
                        ft.Icon(ft.Icons.PHONE_DISABLED_ROUNDED, size=48, color=TEXT_MUTED),
                        ft.Text("iPhone не найден", size=20, weight=ft.FontWeight.W_700),
                        ft.Text("Подключите устройство по USB и доверьте этому компьютеру.", size=13, color=TEXT_MUTED),
                    ],
                ),
                expand=True,
            )
        else:
            catalog_rows = self.installed_catalog_apps()
            body = ft.Column(
                expand=True,
                spacing=14,
                controls=[
                    ft.ResponsiveRow(
                        spacing=14,
                        run_spacing=14,
                        controls=[
                            self.metric("UDID", info.udid, ft.Icons.FINGERPRINT_ROUNDED),
                            self.metric("Модель", info.product_type or info.device_name or "iPhone", ft.Icons.PHONE_IPHONE_ROUNDED),
                            self.metric("iOS", info.product_version or "unknown", ft.Icons.SYSTEM_UPDATE_ROUNDED),
                            self.metric("Батарея", f"{info.battery_current_capacity}%" if info.battery_current_capacity is not None else "unknown", ft.Icons.BATTERY_CHARGING_FULL_ROUNDED),
                        ],
                    ),
                    glass_container(
                        ft.Column(
                            spacing=8,
                            controls=[
                                ft.Text("Приложения Signum на iPhone", size=18, weight=ft.FontWeight.W_700),
                                ft.Column(
                                    spacing=6,
                                    controls=[
                                        ft.Row(
                                            alignment=ft.MainAxisAlignment.SPACE_BETWEEN,
                                            controls=[
                                                ft.Column(
                                                    spacing=1,
                                                    controls=[
                                                        ft.Text(app.title, color="#EAF0FF"),
                                                        ft.Text(bundle, color=TEXT_MUTED, size=11),
                                                    ],
                                                ),
                                                ft.Text(installed.version or "unknown", color=TEXT_MUTED),
                                            ],
                                        )
                                        for app, bundle, installed in catalog_rows
                                    ]
                                    or [ft.Text("Банковские приложения из каталога не найдены.", color=TEXT_MUTED)],
                                ),
                            ],
                        ),
                        expand=True,
                    ),
                ],
            )
        self.content.content = ft.Column(
            expand=True,
            spacing=18,
            controls=[
                self.view_header(
                    "Устройство",
                    "USB-диагностика, iOS и приложения Signum",
                    [icon_button(ft.Icons.REFRESH_ROUNDED, "Обновить устройство", lambda _: asyncio.create_task(self.refresh_device()))],
                ),
                body,
            ],
        )

    def metric(self, title: str, value: str, icon: str) -> ft.Control:
        return ft.Container(
            col={"xs": 12, "sm": 6, "md": 3},
            content=glass_container(
                ft.Column(
                    spacing=12,
                    controls=[
                        ft.Icon(icon, color=ACCENT_2, size=25),
                        ft.Text(title, color=TEXT_MUTED, size=12),
                        ft.Text(value, color="#F5F7FF", size=16, weight=ft.FontWeight.W_700, max_lines=2, overflow=ft.TextOverflow.ELLIPSIS),
                    ],
                ),
                height=150,
            ),
        )

    def render_account(self) -> None:
        session = self.store.get_auth_session()
        self.content.content = ft.Column(
            expand=True,
            spacing=18,
            controls=[
                self.view_header("Аккаунт", "Apple ID используется только для официальной загрузки через App Store"),
                glass_container(
                    ft.Column(
                        spacing=18,
                        controls=[
                            ft.Row(
                                alignment=ft.MainAxisAlignment.SPACE_BETWEEN,
                                controls=[
                                    ft.Column(
                                        spacing=4,
                                        controls=[
                                            ft.Text("Сессия", size=18, weight=ft.FontWeight.W_700),
                                            ft.Text("Активна" if session else "Не выполнена", color=SUCCESS if session else TEXT_MUTED),
                                            ft.Text(session.email if session else "Apple ID не подключен", color=TEXT_MUTED),
                                        ],
                                    ),
                                    ft.FilledButton("Войти", icon=ft.Icons.LOGIN_ROUNDED, on_click=lambda _: asyncio.create_task(self.show_auth_dialog()), style=ft.ButtonStyle(shape=ft.RoundedRectangleBorder(radius=8))),
                                ],
                            ),
                            ft.Divider(color="#22FFFFFF"),
                            ft.Text("Пароль не сохраняется в приложении. Локально хранится только отметка успешной сессии и служебный кэш.", color=TEXT_MUTED, size=13),
                        ],
                    ),
                    expand=True,
                ),
            ],
        )

    def render_telegram(self) -> None:
        self.content.content = ft.Column(
            expand=True,
            spacing=18,
            controls=[
                self.view_header("AppBank", "Telegram-канал с релизами и статусами приложений"),
                glass_container(
                    ft.Column(
                        spacing=16,
                        controls=[
                            ft.Icon(ft.Icons.SEND_ROUNDED, size=54, color=ACCENT_2),
                            ft.Text("Telegram: AppBank", size=26, weight=ft.FontWeight.W_700),
                            ft.Text("Новости, релизы и обновления банковских приложений.", color=TEXT_MUTED),
                            ft.FilledButton("Открыть", icon=ft.Icons.OPEN_IN_NEW_ROUNDED, url="https://t.me/AppBank", style=ft.ButtonStyle(shape=ft.RoundedRectangleBorder(radius=8))),
                        ],
                    ),
                    expand=True,
                ),
            ],
        )

    async def refresh_apps(self) -> None:
        try:
            self.apps = await self.api.fetch_apps()
            await self.updater.capture_known_apps(self.apps)
            if self.closed:
                return
            if self.selected_index == 0:
                self.render_home()
            elif self.selected_index == 1:
                self.render_device()
            self.safe_update()
        except RuntimeError:
            self.closed = True
        except Exception as exc:
            if self.closed:
                return
            self.apps = []
            if self.selected_index == 0:
                self.render_home()
            show_snack(self.page, f"Каталог недоступен: {exc}", color=ERROR)

    async def refresh_device(self) -> None:
        try:
            self.device_info = await self.device.get_device_info()
            self.installed_apps = await self.device.list_installed_apps()
            if self.closed:
                return
        except Exception as exc:
            if self.closed:
                return
            self.device_info = None
            self.installed_apps = {}
            show_snack(self.page, f"Устройство не найдено: {exc}", color=ERROR)
        if self.selected_index == 1:
            self.render_device()
            self.safe_update()

    async def open_app_detail(self, app: AppSummary) -> None:
        try:
            info = await self.api.fetch_app_info(app.id)
            self.app_infos[app.id] = info
        except Exception as exc:
            show_snack(self.page, f"Не удалось открыть приложение: {exc}", color=ERROR)
            return

        can_install, reason = self.check_compatibility(info)
        installed_app = self.installed_app_for(info)
        install_button = ft.FilledButton(
            "Переустановить" if installed_app else "Установить",
            icon=ft.Icons.SYNC_ROUNDED if installed_app else ft.Icons.DOWNLOAD_ROUNDED,
            disabled=not can_install,
            style=ft.ButtonStyle(shape=ft.RoundedRectangleBorder(radius=8)),
        )
        status_text = ft.Text(self.availability_text(info), color=self.availability_color(info))
        bundle_text = ft.Text(self.bundle_id_for(info) or "bundle_id не указан", color=TEXT_MUTED)
        installed_text = ft.Text(self.installed_status_text(info), color=SUCCESS if installed_app else TEXT_MUTED)
        compatibility_text = ft.Text(reason or "Приложение совместимо с подключенным iPhone.", color=SUCCESS if can_install else ERROR)
        latest_text = ft.Text("Для скачивания используется последняя доступная версия по app_store_id.", color=TEXT_MUTED)

        install_button.on_click = lambda _: asyncio.create_task(self.install_from_catalog(info))

        content = ft.Column(
            width=520,
            tight=True,
            spacing=14,
            controls=[
                ft.Row(
                    spacing=14,
                    controls=[
                        ft.Image(src=info.icon_url or app.icon_url, width=72, height=72, fit=ft.BoxFit.COVER, border_radius=16) if (info.icon_url or app.icon_url) else ft.Icon(ft.Icons.ACCOUNT_BALANCE_ROUNDED, size=58, color=ACCENT_2),
                        ft.Column(
                            spacing=4,
                            controls=[
                                ft.Text(info.title or app.title, size=23, weight=ft.FontWeight.W_700),
                                bundle_text,
                                ft.Text(f"Текущая версия: {info.version or 'не указана'}", color=TEXT_MUTED),
                                status_text,
                                installed_text,
                            ],
                        ),
                    ],
                ),
                ft.Divider(color="#22FFFFFF"),
                ft.Text(f"Скачивание: последняя версия{f' {info.version}' if info.version else ''}", size=15, weight=ft.FontWeight.W_700),
                latest_text,
                compatibility_text,
                self.progress_text,
            ],
        )
        self.page.dialog = ft.AlertDialog(
            modal=True,
            bgcolor=PANEL,
            shape=ft.RoundedRectangleBorder(radius=8),
            content=content,
            actions=[plain_action_button("Закрыть", ft.Icons.CLOSE_ROUNDED, self.close_dialog), install_button],
        )
        self.current_dialog = self.page.dialog
        self.page.show_dialog(self.page.dialog)
        self.page.update()
        if not can_install:
            show_snack(self.page, reason or "Несовместимая версия iOS", color=ERROR)

    def default_version(self, info: AppInfo) -> AppVersion | None:
        for version in info.versions:
            if version.is_latest:
                return version
        return info.versions[0] if info.versions else None

    def build_version_controls(
        self,
        info: AppInfo,
        selected: AppVersion | None,
        on_change: Callable[[AppVersion | None], None],
    ) -> ft.Control:
        if not info.versions:
            return glass_container(
                ft.Row(
                    alignment=ft.MainAxisAlignment.SPACE_BETWEEN,
                    controls=[
                        ft.Text(info.version or "latest", weight=ft.FontWeight.W_700),
                        ft.Text("Новая версия", color=ACCENT_2),
                    ],
                ),
                padding=12,
            )

        radio = ft.RadioGroup(
            value=selected.version if selected else info.versions[0].version,
            on_change=lambda e: on_change(next((item for item in info.versions if item.version == e.control.value), None)),
            content=ft.Column(
                spacing=8,
                controls=[
                    glass_container(
                        ft.Row(
                            alignment=ft.MainAxisAlignment.SPACE_BETWEEN,
                            controls=[
                                ft.Row(
                                    spacing=8,
                                    controls=[
                                        ft.Radio(value=item.version),
                                        ft.Column(
                                            spacing=2,
                                            controls=[
                                                ft.Text(item.label, weight=ft.FontWeight.W_700),
                                                ft.Text(f"Версия {item.version} · iOS {item.min_ios or info.min_ios or 'any'} · {self.availability_text(info, item)}", color=TEXT_MUTED, size=12),
                                            ],
                                        ),
                                    ],
                                ),
                                ft.Container(
                                    padding=ft.Padding(left=8, right=8, top=4, bottom=4),
                                    border_radius=8,
                                    bgcolor="#223FE28F" if item.is_latest else "#227C5CFF",
                                    content=ft.Text("новая" if item.is_latest else "старая", size=11, color="#FFFFFF"),
                                ),
                            ],
                        ),
                        padding=10,
                    )
                    for item in info.versions
                ],
            ),
        )
        return radio

    def close_dialog(self, _: ft.ControlEvent | None = None) -> None:
        if self.current_dialog:
            self.current_dialog.open = False
        try:
            if self.page.dialog:
                self.page.dialog.open = False
        except Exception:
            pass
        self.current_dialog = None
        self.safe_update()

    def check_compatibility(self, info: AppInfo, version: AppVersion | None = None) -> tuple[bool, str | None]:
        min_ios = version.min_ios if version and version.min_ios else info.min_ios
        if not min_ios or not self.device_info or not self.device_info.product_version:
            return True, None
        if compare_versions(self.device_info.product_version, min_ios) < 0:
            return False, f"Нужна iOS {min_ios} или выше. Сейчас iOS {self.device_info.product_version}."
        return True, None

    async def ensure_authenticated(self) -> bool:
        if self.store.get_auth_session():
            return True
        await self.show_auth_dialog()
        return bool(self.store.get_auth_session())

    async def show_auth_dialog(self) -> None:
        email = ft.TextField(label="Apple ID", border_radius=8, autofocus=True)
        password = ft.TextField(label="Пароль", password=True, can_reveal_password=True, border_radius=8)
        code = ft.TextField(label="Код подтверждения", border_radius=8, max_length=6, visible=False)
        status = ft.Text("", color=TEXT_MUTED)
        login_button = ft.FilledButton(
            "Войти",
            icon=ft.Icons.LOGIN_ROUNDED,
            style=ft.ButtonStyle(shape=ft.RoundedRectangleBorder(radius=8)),
        )

        async def do_login(_: ft.ControlEvent) -> None:
            login_button.disabled = True
            status.value = "Проверяем учетную запись..."
            status.color = TEXT_MUTED
            self.page.update()
            try:
                auth_code = code.value if code.visible else None
                await self.ipatool.login(email.value or "", password.value or "", auth_code)
                self.render_account()
                self.close_dialog()
                show_snack(self.page, "Apple ID подключен", color=SUCCESS)
            except Exception as exc:
                message = self.format_error(exc)
                if not code.visible and self.is_two_factor_error(message):
                    code.visible = True
                    code.autofocus = True
                    login_button.text = "Подтвердить"
                    status.value = "Apple запросил код подтверждения. Введите код и нажмите «Подтвердить»."
                    status.color = WARNING
                else:
                    status.value = message
                    status.color = ERROR
                login_button.disabled = False
                self.page.update()

        login_button.on_click = lambda e: asyncio.create_task(do_login(e))
        self.page.dialog = ft.AlertDialog(
            modal=True,
            bgcolor=PANEL,
            shape=ft.RoundedRectangleBorder(radius=8),
            title=ft.Text("Вход в Apple ID"),
            content=ft.Column(width=420, tight=True, spacing=12, controls=[email, password, code, status]),
            actions=[
                plain_action_button("Отмена", ft.Icons.CLOSE_ROUNDED, self.close_dialog),
                login_button,
            ],
        )
        self.current_dialog = self.page.dialog
        self.page.show_dialog(self.page.dialog)
        self.page.update()

    @staticmethod
    def is_two_factor_error(message: str) -> bool:
        lowered = message.lower()
                                                                         
        markers = (
            "signum_2fa_required",
            "2fa code is required",
            "auth code is required",
            "two-factor authentication is required",
            "two factor authentication is required",
            "secondary authentication required",
            "enter the verification code",
            "запросил код подтверждения",
            "требуется код подтверждения",
            "нужен код подтверждения",
        )
        return any(marker in lowered for marker in markers)

    @staticmethod
    def format_error(exc: Exception) -> str:
        if isinstance(exc, TimeoutError) or isinstance(exc, asyncio.TimeoutError):
            return "Операция заняла слишком много времени. Проверьте подключение iPhone и повторите попытку."
        message = str(exc).strip()
        if not message:
            return "Неизвестная ошибка. Попробуйте ещё раз."
        lowered = message.lower()
        if "cookies.lock" in lowered or "cannot acquire lock" in lowered:
            return "ipatool сейчас занят или остался lock-файл. Signum попробовал очистить его автоматически. Повторите вход через минуту."
        if "anisette" in lowered:
            return (
                "Не удалось запустить anisette.exe. Проверьте, что файл лежит рядом с ipatool.exe, "
                "не заблокирован Защитником Windows. Для ipatool v3 на Windows также нужен iCloud."
            )
        if "locked" in lowered or "iforgot" in lowered or "-20209" in lowered:
            return "Apple заблокировал этот Apple ID. Разблокируйте на https://iforgot.apple.com и повторите вход."
        if "account is disabled" in lowered or ("disabled" in lowered and "account" in lowered):
            return "Этот Apple ID отключён. Проверьте статус аккаунта на appleid.apple.com."
        if ("incorrect" in lowered or "invalid" in lowered) and (
            "password" in lowered or "credential" in lowered or "account" in lowered
        ):
            return "Неверный Apple ID или пароль."
        if "app not found" in lowered:
            return "ipatool не нашел приложение. Проверьте bundle_id и app_store_id в каталоге."
        return message[:900]

    async def install_from_catalog(self, info: AppInfo, version: AppVersion | None = None) -> None:
        bundle_id = info.bundle_id
        app_store_id = info.app_store_id
        if not bundle_id:
            show_snack(self.page, "Не хватает данных для установки приложения", color=ERROR)
            return
        ok, reason = self.check_compatibility(info)
        if not ok:
            show_snack(self.page, reason or "Несовместимая версия iOS", color=ERROR)
            return
        try:
            self.set_progress(True, "Проверка iPhone", 0.04)
            self.installed_apps = await self.device.list_installed_apps()
            installed_app = self.installed_apps.get(bundle_id)
            self.set_progress(False, "", 0)
        except Exception as exc:
            message = self.format_error(exc)
            self.set_progress(False, f"iPhone не найден: {message}", 0, ERROR)
            show_snack(self.page, f"iPhone не найден: {message}", color=ERROR)
            return
        if not await self.ensure_authenticated():
            self.set_progress(False, "", 0)
            return
        try:
            if installed_app:
                installed_version = f" {installed_app.version}" if installed_app.version else ""
                self.set_progress(True, f"На iPhone найдено: {bundle_id}{installed_version}", 0.06)
            if self.version_in_store(info):
                self.set_progress(True, "Привязка приложения к аккаунту", 0.08)
                await self.ipatool.purchase(bundle_id)
            else:
                self.set_progress(True, "Поиск приложения в покупках", 0.08)
            self.set_progress(True, "Загрузка IPA", 0.18)
            ipa_path = await self.ipatool.download(
                bundle_id,
                app_store_id=app_store_id,
                progress=self.on_download_progress,
            )
            self.set_progress(True, "Установка на iPhone", 0.84)
            await self.device.install_ipa(ipa_path, progress=self.on_install_progress)
            self.set_progress(False, "Готово", 1)
            show_snack(self.page, f"{info.title or bundle_id} установлено", color=SUCCESS)
            await self.refresh_device()
        except Exception as exc:
            message = self.format_error(exc)
            self.set_progress(False, f"Установка не удалась: {message}", 0, ERROR)
            show_snack(self.page, f"Установка не удалась: {message}", color=ERROR)

    def on_download_progress(self, percent: float, line: str) -> None:
        value = max(0.18, min(0.82, percent / 100 * 0.64 + 0.18))
        self.set_progress(True, line or "Загрузка IPA", value)

    def on_install_progress(self, percent: float, line: str) -> None:
        value = max(0.84, min(1, percent / 100))
        self.set_progress(True, line or "Установка на iPhone", value)

    def set_progress(self, visible: bool, text: str, value: float, color: str = TEXT_MUTED) -> None:
        if self.closed:
            return
        self.progress.visible = visible
        self.progress.value = value
        self.progress_text.value = text
        self.progress_text.color = color
        try:
            self.page.update()
        except RuntimeError:
            self.closed = True

    async def open_ipa_picker(self) -> None:
        files = await self.file_picker.pick_files(
            allow_multiple=False,
            allowed_extensions=["ipa"],
            file_type=ft.FilePickerFileType.CUSTOM,
            dialog_title="Выберите IPA для установки",
        )
        if not files:
            return
        path = Path(files[0].path)
        try:
            self.set_progress(True, "Установка локального IPA", 0.5)
            await self.device.install_ipa(path, progress=self.on_install_progress)
            self.set_progress(False, "Готово", 1)
            show_snack(self.page, f"{path.name} установлен", color=SUCCESS)
            await self.refresh_device()
        except Exception as exc:
            message = self.format_error(exc)
            self.set_progress(False, f"Не удалось установить IPA: {message}", 0, ERROR)
            show_snack(self.page, f"Не удалось установить IPA: {message}", color=ERROR)

    async def check_ota_update(self, *, silent: bool, auto_install: bool = False) -> None:
        try:
            update = await self.api.fetch_ota_update()
        except Exception as exc:
            if not silent:
                show_snack(self.page, f"OTA недоступен: {exc}", color=ERROR)
            return
        if not update.has_update:
            if not silent:
                show_snack(self.page, "Установлена последняя версия Signum", color=SUCCESS)
            return
        if auto_install:
            await self.install_ota_update(update, automatic=True)
            return
        self.show_ota_banner(update)

    async def install_ota_update(self, update: OtaUpdate, *, automatic: bool = False) -> None:
        if not self.ota.can_self_update:
            if not automatic:
                show_snack(self.page, "Автообновление работает в собранном Windows .exe", color=WARNING)
            return
        try:
            self.set_progress(True, "Загрузка обновления Signum", 0.05)
            downloaded = await self.ota.download_update(update, progress=self.on_ota_progress)
            self.set_progress(True, "Установка обновления Signum", 1)
            await self.ota.install_update_and_restart(downloaded)
        except Exception as exc:
            self.set_progress(False, f"Не удалось обновить Signum: {exc}", 0, ERROR)
            if not automatic:
                show_snack(self.page, f"Не удалось обновить Signum: {exc}", color=ERROR)

    def show_ota_banner(self, update: OtaUpdate) -> None:
        async def install(_: ft.ControlEvent) -> None:
            self.dismiss_banner()
            await self.install_ota_update(update, automatic=False)

        self.page.banner = ft.Banner(
            bgcolor="#EE161A26",
            leading=ft.Icon(ft.Icons.AUTO_MODE_ROUNDED, color=ACCENT_2),
            content=ft.Text(f"Доступна новая версия Signum {update.version or ''} build {update.build or ''}. {update.notes or ''}"),
            actions=[
                ft.TextButton("Позже", disabled=update.mandatory, on_click=lambda _: self.dismiss_banner()),
                ft.FilledButton("Обновить", on_click=lambda e: asyncio.create_task(install(e)), style=ft.ButtonStyle(shape=ft.RoundedRectangleBorder(radius=8))),
            ],
        )
        self.page.banner.open = True
        self.page.update()

    def on_ota_progress(self, percent: float, line: str) -> None:
        self.set_progress(True, line or "Загрузка обновления Signum", max(0.05, min(1, percent / 100)))

    async def on_update_found(self, candidate: UpdateCandidate) -> None:
        async def install(_: ft.ControlEvent) -> None:
            self.dismiss_banner()
            await self.install_from_catalog(candidate.info)

        self.page.banner = ft.Banner(
            bgcolor="#EE161A26",
            leading=ft.Icon(ft.Icons.SYSTEM_UPDATE_ALT_ROUNDED, color=ACCENT_2),
            content=ft.Text(f"Вышла новая версия {candidate.title}: {candidate.installed_version} -> {candidate.remote_version}"),
            actions=[
                ft.TextButton("Позже", on_click=lambda _: self.dismiss_banner()),
                ft.FilledButton("Обновить", on_click=lambda e: asyncio.create_task(install(e)), style=ft.ButtonStyle(shape=ft.RoundedRectangleBorder(radius=8))),
            ],
        )
        self.page.banner.open = True
        self.page.update()

    def dismiss_banner(self) -> None:
        if self.page.banner:
            self.page.banner.open = False
            self.page.update()


async def main(page: ft.Page) -> None:
    app = SignumApp(page)
    await app.start()


if __name__ == "__main__":
    ft.run(main, name="Signum", assets_dir=str(RESOURCE_DIR / "assets"))
