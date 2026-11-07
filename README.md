# Signum — Tauri port

Новая версия Signum находится в корне репозитория. Исходная Python/Flet-версия сохранена рядом как эталон поведения.

## Архитектура

```text
React pages/components
        │ typed invoke + Tauri events
        ▼
Rust commands ──► Signum PHP API
        │
        ├──► ipatool (Apple ID, purchase, download)
        └──► libimobiledevice (pair, info, list, install, archive)
```

- React отвечает только за отображение, маршруты и локальное состояние форм.
- HTTP к `main.php`, `info.php` и `ota.php` выполняет Rust; CORS фронтенду не нужен.
- Команды установки не блокируют WebView: они добавляют задания в Rust-очередь и сразу возвращают `jobId`.
- Один backend worker последовательно выполняет `purchase → download → install`. Это исключает конфликт `cookies.lock` и конкурирующие USB-операции.
- stdout/stderr процессов читаются асинхронно. Rust извлекает проценты и посылает `install-progress`; страница «Установки» подписана через `listen`.
- Пароль Apple ID передаётся по локальному Tauri IPC только в момент входа, не логируется и не сохраняется. Служебная passphrase ipatool хранится в Windows Credential Manager/macOS Keychain через `keyring`.
- Настройки лежат в системной папке конфигурации приложения; IPA-кэш — в app data, если пользователь не выбрал другой путь.

## Разработка

Требуются Node.js 20+, Rust stable и системные зависимости Tauri 2.

```powershell
npm install
npm run tauri dev
```

### Сборки по ОС (каждая платформа — на своей машине)

| ОС | Где собирать | Команда | Артефакт |
|----|----------------|---------|----------|
| **Windows** | Windows | `npm run build:win` | `SignumSetup.exe` (свой установщик) |
| **macOS** | Mac | `npm run build:mac` или `scripts/build-mac.sh` | `.dmg` |
| **Linux** | Linux (не WSL как основной путь) | `npm run build:linux` или `scripts/build-linux.sh` | `.AppImage`, `.deb` |

CI: `.github/workflows/build.yml` — `windows-latest` + `macos-latest` + `ubuntu-22.04`.

```powershell
# Windows
npm run build:win
```

```bash
# macOS (только на Mac)
./scripts/build-mac.sh
# или: npm run build:mac
```

```bash
# Linux (только на Linux)
./scripts/build-linux.sh
# или: npm run build:linux
```

## Android

Нативное приложение: каталог, вход в Apple ID, поиск приложений, iPhone по USB. **Фонового поддержания сессии покупок нет.**

```text
android/          # Android Studio → Open
```

```powershell
cd android
.\gradlew.bat :app:assembleDebug
# APK: android\app\build\outputs\apk\debug\app-debug.apk
```

Нужны кабель OTG и разрешение USB. На iPhone — «Доверять».

`ipatool` / `idevice*` на Windows бандлятся из `apple-tools/windows-x64/`.

**macOS:**
- `brew install libimobiledevice ideviceinstaller`
- ipatool в ресурсах: `apple-tools/macos-arm64/v2/`, `apple-tools/macos-x64/v2/`

**Linux:**
- `usbmuxd`, `libimobiledevice-utils`, `ideviceinstaller` (apt)
- положите Linux-бинарь `ipatool` в `linux_x64/` (x86_64) или `linux_arm64/`
- для входа и App Store restore нужен Linux-compatible `sapsigner` рядом с `ipatool`
- без бандла Signum ищет `ipatool` / `idevice*` в PATH

**Windows:**
- инструменты из `apple-tools/windows-x64/`
- Apple Mobile Device Support / iTunes / iCloud — для USB и anisette

## Основные файлы

- `src/App.tsx` — layout routing.
- `src/pages/*` — каталог, устройства, аккаунт, очередь и настройки.
- `src/components/AppCard.tsx` — карточка приложения с многослойной тенью.
- `src-tauri/src/main.rs` — API, credential storage, процессы, очередь и события.
- `src-tauri/tauri.conf.json` — frameless window, ресурсы и DMG. Windows ставится своим установщиком (`src-setup/`), не NSIS.
