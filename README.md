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

CI: `.github/workflows/build-linux.yml`, `build-macos.yml`, `build-mac-tools.yml` (macOS-сборка и нативные `kbsync`/`zsign`/device-tools) и `build-mac-app.yml`. Windows-инструменты собираются в форке `colossalblend/ipatool`.

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

Для Android в `android/app/src/main/jniLibs/arm64-v8a/` нужен `libsapsigner.so`, а в `android/app/src/main/assets/sap/` — `CommerceKit`, `storeagent`, `CommerceCore`, `CoreFP`, `CoreFP.icxs`. Это поставляемые отдельно бинарные компоненты (см. `ANDROID.md`), в репозиторий они не коммитятся.

## Сторонние компоненты и источники

### Apple ID, App Store, подпись

| Компонент | Версия | Откуда берётся | Назначение |
|-----------|--------|----------------|------------|
| **ipatool-cpp** (движок v5) | 1.3.4 | [Sorvigolova/ipatool](https://github.com/Sorvigolova/ipatool/releases/tag/1.3.4); бинарники Windows/macOS/Linux совпадают побайтно с [kda2495/IPA_Downloader 4.5.1](https://github.com/kda2495/IPA_Downloader/releases/tag/4.5.1) | вход в Apple ID, покупка, `list-versions`, скачивание IPA; SAP/KBSync внутри бинаря |
| **ipatool** Go (движок v4) | majd/ipatool 2.6.0 + sapfix | [majd/ipatool](https://github.com/majd/ipatool), [maksimryabkin/ipatool](https://github.com/maksimryabkin/ipatool) (`2.3.2-sapfix.1`) | резервный движок |
| **KBSync / sapsigner** | Unicorn 2.1.4 | [t0rr3sp3dr0/sapsigner](https://github.com/t0rr3sp3dr0/sapsigner) (Go + Unicorn) | FairPlay и `X-Apple-ActionSignature` |
| **Unicorn Engine** | 2.1.4 | [unicorn-engine/unicorn](https://github.com/unicorn-engine/unicorn), `brew install unicorn` | эмуляция для `kbsync`/`sapsigner`; на Windows DLL кладётся в `%LOCALAPPDATA%\ipatool\unicorn\2.1.4` |
| **zsign** | upstream | [zhlynn/zsign](https://github.com/zhlynn/zsign) (`build/macos`) | переподпись IPA |
| **anisette** | 3.0.0-legacy | legacy-sidecar из ipatool 3.0.0 | только для старого движка v3 |
| **IPA_Downloader** | 4.5.1 | [kda2495/IPA_Downloader](https://github.com/kda2495/IPA_Downloader/releases/tag/4.5.1) | актуальные `ideviceinfo`, `ideviceinstaller`, `idevice_id`, `idevicepair`; Wi‑Fi-подключение с приоритетом USB |
| **libimobiledevice / libplist / usbmuxd** | brew/apt | [libimobiledevice](https://github.com/libimobiledevice) | USB: pairing, info, install |
| **isideload** | 0.2.25 | [nab138/isideload](https://github.com/nab138/isideload), вендорен в `src-tauri/vendor/isideload` | sideload/установка из Rust |
| **Apple Mobile Device Support / iTunes / iCloud** | — | apple.com | USB и anisette на Windows |

Контрольные суммы и версии разложенных инструментов — в `apple-tools/*/SOURCE.json`.

### Десктопный стек

- **Frontend:** React 18.3, React Router 7, Vite 6, TypeScript 5.6, Tailwind CSS 3.4, lucide-react, qrcode, javascript-obfuscator.
- **Rust / Tauri 2:** `tauri`, `tauri-plugin-dialog`, `tauri-plugin-notification`, `tauri-plugin-single-instance`, `tokio`, `reqwest` (rustls/native-tls/socks), `serde`, `keyring` 3, `aes-gcm`, `aes`/`cbc`, `hmac`, `pbkdf2`, `sha1`/`sha2`, `rustls`, `rcgen`, `plist`, `zip`, `flate2`, `num-bigint`, `uuid`, `regex`, `base64`, `futures-util`, `tracing`.
- **Python-эталон (`main.py`, `signum/`):** Flet ≥ 0.24.1, flet-desktop, httpx, keyring, packaging (`requirements.txt`).

### Android

- AGP 9.3.1, Kotlin 2.2.10, Compose BOM 2026.02.01, Navigation Compose 2.9.6, Lifecycle 2.11.
- OkHttp 4.12, Coil 2.7, kotlinx-serialization 1.8.1, BouncyCastle 1.80, dd-plist 1.28, ZXing 3.5.4, androidx security-crypto, documentfile.
- `libsapsigner.so` собирается из [t0rr3sp3dr0/sapsigner](https://github.com/t0rr3sp3dr0/sapsigner) с Unicorn; SAP-компоненты Apple поставляются отдельно.

## Что скачивается при сборке и работе

- **npm:** `npm install` → registry.npmjs.org (список — `package.json` / `package-lock.json`).
- **Rust:** `cargo` → crates.io; `isideload` берётся из `src-tauri/vendor/isideload`.
- **ipatool-cpp для macOS:** `gh release download 1.3.4 -R Sorvigolova/ipatool -p 'macOS_*_ipatool-cpp.zip'` (проверка SHA-256 в `.github/workflows/build-macos.yml`).
- **ipatool-cpp для Windows:** собирается в GitHub Actions форка `colossalblend/ipatool` (windows-2022, Visual Studio 2022 + vcpkg static x64).
- **macOS device-tools / kbsync / zsign:** `.github/workflows/build-mac-tools.yml` — `brew install libimobiledevice ideviceinstaller unicorn`, клон `t0rr3sp3dr0/sapsigner`, `zhlynn/zsign`; зависимости перепривязываются на `@executable_path`.
- **Обновление движков без переустановки:** манифест `backend/signum/files/tools/manifest.json` и API; файлы проверяются по SHA-256 и кладутся в папку конфигурации приложения (`tools/`), IPA-кэш — в app data.
- **Unicorn DLL на Windows:** копируется из ресурсов рядом с `ipatool.exe` в кэш `%LOCALAPPDATA%\ipatool\unicorn\2.1.4\<sha256>\`.
- **Android:** Gradle/Maven Central; `libsapsigner.so` и SAP-ассеты — из локальной сборки (в репозиторий не входят).

## Основные файлы

- `src/App.tsx` — layout routing.
- `src/pages/*` — каталог, устройства, аккаунт, очередь и настройки.
- `src/components/AppCard.tsx` — карточка приложения с многослойной тенью.
- `src-tauri/src/main.rs` — API, credential storage, процессы, очередь и события.
- `src-tauri/tauri.conf.json` — frameless window, ресурсы и DMG. Windows ставится своим установщиком (`src-setup/`), не NSIS.
