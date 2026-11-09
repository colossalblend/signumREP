# Signum Handoff

Дата: 2026-11-09

Этот файл передаёт следующему ИИ контекст текущей работы по Signum, `ipatool`, IPA_Downloader и отчётам пользователей.

## Запрос пользователя

Пользователь просил:

- сравнить `majd/ipatool` и `Sorvigolova/ipatool`;
- проверить commit `d5050b1604b8b8964061e59c789ba975b4a68691`;
- собрать Windows-версию через GitHub Actions;
- интегрировать рабочий вариант в Signum;
- изучить `kda2495/IPA_Downloader`, особенно вход в Apple ID, выход и установку IPA;
- исправить долгий выход, проблемы установки и затем подготовить исходники для передачи другому ИИ.

Пользователь отдельно сказал: **сборку Signum пока не запускать; сначала доделать всё, потом собрать**.

## Репозитории

- Signum source: `D:\Signum-v1`
- Upstream Go ipatool: `https://github.com/majd/ipatool`
- Upstream C++ ipatool: `https://github.com/Sorvigolova/ipatool`
- IPA Downloader: `https://github.com/kda2495/IPA_Downloader`
- Fork для GitHub Actions: `https://github.com/colossalblend/ipatool`
- Signum source mirror: `https://github.com/colossalblend/signumREP`

В этой среде нет локального Git CLI. GitHub используется через `gh api` и `gh workflow`.

Последний опубликованный commit SignumRep на момент передачи:

```text
c66032c46760532c3b7eb464014bd92ebd51c634
```

URL: `https://github.com/colossalblend/signumREP/commit/c66032c46760532c3b7eb464014bd92ebd51c634`

## Что проверено

### ipatool

`majd/ipatool` — отдельный Go-инструмент.

`Sorvigolova/ipatool` — C++20 ipatool с SAP, KBSync, Store ADI/AMD и FairPlay emulation.

Commit `d5050b1604b8b8964061e59c789ba975b4a68691` подтверждён как:

```text
Restore native Store authentication and IPA downloads
```

В нём появились native Store authentication, Store ADI proof и AArch64 Unicorn.

### GitHub Actions source build

В fork `colossalblend/ipatool` добавлен ручной workflow:

```text
.github/workflows/build-windows-source.yml
```

Последний успешный запуск:

- URL: `https://github.com/colossalblend/ipatool/actions/runs/42668812874`
- source input: `d5050b1604b8b8964061e59c789ba975b4a68691`
- runner: `windows-2022`
- toolchain: Visual Studio 2022 + vcpkg static x64
- artifact: `ipatool-windows-x64-d5050b1604b8b8964061e59c789ba975b4a68691`
- artifact id: `13698401085`
- built EXE size: `70,266,368` bytes
- built EXE SHA-256: `d0603a9c0ca2d143a9c45e0dd6b1964852b5432f2567983effd374609b70d8ce`

Первые попытки workflow были исправлены:

- `windows-latest` не нашёл Visual Studio;
- `windows-2022` нашёл несовместимый CMake target vcpkg minizip;
- проверка CLI ошибочно считала exit code 1 ошибкой;
- финальная проверка заменена на PE-header проверку.

## Текущие локальные изменения Signum

Изменённые файлы в `D:\Signum-v1`:

- `src-tauri/src/main.rs`
- `apple-tools/SOURCE.json`
- `apple-tools/windows-x64/SOURCE.json`
- `scripts/publish_tools_release.ps1`
- `backend/signum/files/tools/manifest.json`
- `scripts/build-ipatool-windows-actions.yml`
- `HANDOFF-SIGNUM-2026-11-09.md`

### Реализованные изменения в `main.rs`

1. Windows managed ipatool v5 использует версию `1.3.0`.
2. Windows-specific KBSync version marker поднят до `1.3.0`.
3. Добавлен `KbsyncCancelGuard` и флаг отмены фонового KBSync.
4. Logout, account switch, account removal и login отменяют текущий KBSync warm-up до ожидания общего `ipatool_lock`.
5. Перед массовой проверкой версий каталога убран синхронный `ensure_kbsync_warm()`. KBSync нужен скачиванию, а не `list-versions`; прежний вызов мог блокировать интерфейс на десятки/сотни секунд.
6. Добавлена классификация opaque Apple login responses `301/204/403/404/500`, чтобы не отправлять те же credentials во второй движок после явного ответа Apple.
7. Установка IPA повторяет схему IPA_Downloader: после отказа `install` пробует `upgrade`.
8. `ideviceinstaller`-вывод с `DONE. Installing '...'` / `DONE. Upgrading '...'` распознаётся как успешный install даже если процесс не завершается после предупреждений `.sinf`/`iTunesMetadata.plist`.
9. Добавлены unit tests для opaque Apple response и `ideviceinstaller DONE` marker.
10. Фоновый прогрев KBSync больше не запускается синхронно перед массовым `list-versions` каталога.
11. Logout/login/switch/remove используют `KbsyncCancelGuard`; уже запущенный KBSync child process завершается до ожидания общего ipatool lock.

Важно: после последних правок Signum **не собирался**, потому что пользователь попросил сначала закончить анализ и подготовку.

### Инструменты IPA_Downloader

Проверен релиз `kda2495/IPA_Downloader 4.4.0`:

- `ipatool-cpp` совпадает с upstream Signum 1.3.0 побайтно;
- `idevice_id.exe`, `ideviceinstaller.exe`, `libimobiledevice-1.0.dll`, `libplist-2.0.dll` совпадают с Signum;
- новый `ideviceinfo.exe` имеет тот же upstream version `1.4.0-9-gfa0f791`, но размер 77,824 вместо 317,459 байт;
- новый `ideviceinfo.exe` SHA-256:

```text
92e0aaa390d550b208a200b347643d2dd5112c164c19f14c307634b11b272a64
```

Новый `ideviceinfo.exe` уже заменён в исходниках и установленной копии Signum. Старые файлы сохранены с суффиксом:

```text
ideviceinfo.exe.before-ipa-downloader-4.4.0
```

Provenance обновлён в `apple-tools/windows-x64/SOURCE.json`.

## Важные результаты диагностики

### Долгий выход

В пользовательском `signum-report.txt`:

```text
KBSync generated готов за 172 с
```

Причина: source-built `d5050b1` KBSync на этом ПК генерирует blob около 170 секунд. На том же ПК официальный upstream `1.3.0` занимает около 19 секунд.

Поэтому managed profile был переключён обратно на официальный `ipatool-cpp 1.3.0`:

```text
C:\Users\User\AppData\Roaming\pw.appbank.signum\tools\windows-x64\ipatool-v5-b99082f8d984dc53.exe
```

SHA-256:

```text
b99082f8d984dc53ec3c2b38421aa953455ba38d7fadfbfb7b838db10f57d230
```

Source-built `d5050b1` сохранён рядом:

```text
ipatool-v5-d0603a9c0ca2d143.exe
```

Старые состояния JSON сохранены в managed-каталоге.

### Старый баг warm-kbsync

Исторические отчёты Signum 1.1.x содержат:

```text
Unknown command: warm-kbsync
```

Это баг старого клиента/старого бинаря. Текущий исходный Signum использует `kbsync --refresh`; в финальной сборке обязательно проверить, что никаких `warm-kbsync` в активном пути нет.

### Apple login

В отчётах 1.2.0 повторяются Apple responses:

```text
301, 204, 403, 404, 500
```

Signum раньше после opaque V5 error автоматически запускал V4. Это создавало дополнительные попытки входа и могло привести к rate-limit. В исходнике добавлен stop fallback для этого класса ответов.

Это не всегда ошибка Signum: часть ответов является сетевым/Apple-side отказом. Пользователю нужно показывать «Apple временно не отвечает / подождите», а не обвинять пароль.

### Установка IPA

IPA_Downloader использует:

```text
ideviceinstaller ... install IPA
если ошибка -> ideviceinstaller ... upgrade IPA
```

Signum раньше делал `upgrade` только при пропавшем устройстве. Теперь fallback `install -> upgrade` добавлен в исходник.

В одном старом отчёте `ideviceinstaller` печатал:

```text
WARNING: could not locate iTunesMetadata.plist in archive!
WARNING: could not locate Payload/.../SC_Info/...sinf in archive!
DONE.
Installing 'com.example.app'
```

после чего оставался жить до 360-секундного timeout. Добавлен терминальный DONE-marker, который должен завершать такую операцию как успешную.

Важно: `ipatool-v5-*.exe` не является графическим установщиком. Он скачивает IPA. Установку на iPhone выполняет `ideviceinstaller.exe` из `apple-tools/windows-x64/support`.

### macOS

Отчёты macOS запускались из App Translocation:

```text
/private/var/folders/.../AppTranslocation/.../Signum.app/...
```

Там `.app` read-only, поэтому копирование `sapsigner` рядом с V4 могло завершаться `sapsigner copy failed`. Это отдельная проблема упаковки/запуска macOS, не Windows IPA_Downloader. Следующему ИИ нужно проверить, что V4 sidecar копируется в writable Application Support, а не в translocated bundle.

## Что осталось сделать

1. Закончить статическую проверку `main.rs` после последних правок.
2. Запустить `cargo check` и `cargo test` после подтверждения пользователя, что можно проверять исходники. До этого не собирать installer.
3. Проверить, что `KbsyncCancelGuard` действительно прерывает уже запущенный child process и не сбрасывает флаг раньше времени.
4. Проверить, что `ideviceinstaller DONE` marker не объявляет успех при реальном `ERROR/APIInternalError`.
5. Проверить logout на активной сессии: background KBSync должен завершаться, общий lock не должен ждать 170 секунд.
6. Проверить install локального IPA на реальном iPhone с `install -> upgrade` fallback.
7. Проверить macOS App Translocation и V4 `sapsigner` placement.
8. Только после этого собрать финальный Signum Windows installer.

После добавления последних изменений сборка Signum намеренно не запускалась по прямой просьбе пользователя.

## Запрещено делать без отдельного решения пользователя

- Не запускать финальную сборку и установщик до окончания исправлений.
- Не удалять старые managed ipatool-бинарники и backup-файлы.
- Не коммитить `target/`, `dist/`, `downloads/`, `installers/`, `*.ipa`, `*.apk`, пользовательские логи, cookies, Apple account files, passwords, keychain data или `.env`.
- Не публиковать пользовательские отчёты в открытый репозиторий: они могут содержать IP, пути и диагностические данные.

## Полезные проверки после продолжения

```powershell
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
npm run build
```

Для проверки установленного managed ipatool:

```powershell
$p = "$env:APPDATA\pw.appbank.signum\tools\windows-x64\ipatool-v5.json"
Get-Content $p
```

Для проверки PE- и KBSync smoke test использовать отдельный временный `USERPROFILE`, не переменную `$HOME` PowerShell.
