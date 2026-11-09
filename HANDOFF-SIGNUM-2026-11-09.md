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

---

# Продолжение: проверка и доработка (2026-11-09, облачная сессия Claude Code)

Работа шла в ветке `main` репозитория `colossalblend/signumREP`, в Linux-контейнере с
Git и Rust 1.97. Финальная сборка и установщик **не запускались** — запрет выше в силе.

## Чем проверено

| Проверка | Результат |
| --- | --- |
| `cargo check` (Linux x64) | проходит |
| `cargo check --target x86_64-pc-windows-gnu` (весь код под `cfg(windows)`) | проходит |
| `cargo check --target aarch64-apple-darwin` и `x86_64-apple-darwin` | проходит* |
| `cargo test` (Linux) | 160 прошло, 0 упало, 1 запускается вручную |
| `cargo check --tests` под Windows и macOS | проходит |
| `npm run build` (только интерфейс: `tsc`, `vite build`, обфускация) | проходит |

\* Для macOS на Linux нет SDK, поэтому у одной зависимости (`mac-notification-sys`)
в черновой копии заглушён сборочный скрипт. Код Signum проверен компилятором
полностью, но это проверка типов, а не сборка: линковка и запуск на маке не делались.

До правок `cargo test` падал на двух тестах — как раз на тех, что были добавлены в
этом файле (`opaque_apple_login_status_stops_engine_fallback` и
`ideviceinstaller_done_marker_ends_successful_install`). Они ни разу не запускались.

## Что оказалось неверным в правках из этого файла

### 1. «DONE-marker» объявлял успех до начала установки (п. 8 и п. 4 списка задач)

По исходнику ideviceinstaller (`src/ideviceinstaller.c`):

```text
Copying '<ipa>' to device... DONE.     <- IPA залит на телефон
Installing '<bundle>'                  <- команда установки только что отправлена
Install: CreatingStagingDirectory (5%) <- работает installd
...
Install: Complete                      <- вот это конец установки
ERROR: Install failed. Got error ...   <- отказ приходит сюда же, ПОСЛЕ «Installing»
```

`DONE.` + `Installing '…'` — это начало установки, а не конец. Маркер убивал процесс
в этот момент и возвращал успех: настоящий `ERROR/APIInternalError` приходил бы позже
и уже никем не читался. То есть любая установка, у которой эти строки вставали на
отдельные строки, считалась бы удачной независимо от ответа телефона.

Сделано:

- маркер заменён на строку `Install: Complete` / `Upgrade: Complete` (и старый формат
  `Install - Complete`); при любой строке `ERROR:` он не срабатывает;
- после `Complete` процессу даётся 5 секунд выйти самому, и только потом он
  завершается как успешный — это и есть случай «установилось, но процесс висит»;
- для случая из отчёта (после `Installing` тишина до таймаута 360 с) успех больше не
  угадывается по выводу: Signum спрашивает сам телефон (`ideviceinstaller list`).
  Установка считается состоявшейся, только если этой сборки (bundle id + версия из
  IPA) на телефоне **не было до установки**, а теперь она есть. Для этого перед
  установкой снимается список приложений (ждём не дольше 8 с). Если та же версия
  стояла и раньше (перевыпуск подписи) или снимок снять не удалось — остаётся
  прежняя ошибка таймаута: по списку новую копию от старой не отличить.

### 2. Остановка перехода на второй движок не работала в обычном режиме (п. 6)

`is_apple_opaque_auth_response` искала `status=403` и т. п. в тексте ошибки. Но до
`login_catalog_with_fallback` доходит уже готовое сообщение из `format_login_error`,
а технический хвост с кодом ответа добавляется только при включённых «подробных
ошибках» (по умолчанию выключены). В обычном режиме проверка всегда отвечала «нет»,
и V4 запускался, как и раньше. Плюс строка Go-движка `(HTTP 204, 404, 404)` не
распознавалась вовсе — на этом и падал тест.

Сделано:

- `format_login_error` для ответов 301/204/403/404/500 ставит метку
  `SIGNUM_APPLE_UNAVAILABLE:` — по ней переход на второй движок и останавливается;
- коды читаются в обоих форматах (`status=NNN` и `HTTP NNN, NNN`);
- текст для человека: «Apple сейчас не принимает вход (ответ сервера NNN). Пароль
  менять не нужно — подождите несколько минут и повторите»;
- при вводе кода 2FA такой ответ больше не вызывает немедленный повторный вход
  (раньше строка «request failed» считалась сбоем связи — это была ещё одна попытка);
- метка не попадает на страницу аккаунта (`save_last_catalog_error` её срезает),
  интерфейс срезает её в `src/lib/tauri.ts`.

Обычный `something went wrong` без этих кодов (в том числе `status=200`) ведёт себя
как раньше: второй движок пробуется. В исходном варианте функция из-за приоритета
`||` над `&&` останавливала переход на ЛЮБОЙ `something went wrong` — это шире, чем
описано в этом файле, поэтому сужено до перечисленных кодов.

### 3. «KBSync не нужен для list-versions» — неверно (п. 5 и п. 10)

В `ipatool-cpp` 1.3.0 (и 1.2.0) `cmd_list_versions` зовёт `prepare_kbsync`, а тот при
пустом файле аккаунта **сам порождает blob** (`generate_kbsync`). Без прогрева первый
же опрос версий после входа порождает его внутри себя, с пределом 75 секунд. На
машине, где порождение идёт 170 с, опрос убивают раньше, blob не сохраняется, и
каждый следующий опрос начинает с нуля — список версий не появляется никогда.

При этом жалоба на подвисание была справедлива: старый `ensure_kbsync_warm` раз в час
пересоздавал blob с `--refresh`, даже когда тот был на месте.

Сделано (`ensure_kbsync_present`): перед серией опросов движок вызывается как
`kbsync` **без** `--refresh`. Если blob есть — ответ за доли секунды (`source:
"cache"`), если нет — порождается один раз с пределом 300 с. Результат помнится до
смены аккаунта, так что лишний запуск движка — один на запуск приложения. Ежечасное
обновление осталось фону.

### 4. `KbsyncCancelGuard` (п. 3 и п. 5 списка задач)

Процесс он действительно завершал, но были две дыры:

- флаг был булевым: при наложении команд (выход во время входа, двойной щелчок)
  первая закончившаяся сбрасывала отмену, пока вторая ещё ждала замок. Теперь это
  счётчик;
- после `kill` код без предела ждал закрытия stdout/stderr. Канал закрывается, только
  когда его отпустят все владельцы; если движок успел запустить соседний процесс
  (отдельный `kbsync` на macOS/Linux), ожидание длилось бы до его конца — то есть те
  самые минуты. Теперь ожидание ограничено 2 секундами (`drain_killed_child_output`).

Заодно: отмена теперь прерывает и серию опросов версий (остаток помечается
«не проверено», кеш при этом не пишется), иначе выход ждал бы до 75 с на каждую
строку каталога.

На Windows с 1.3.0 KBSync собран внутрь `ipatool.exe`, поэтому `kill` останавливает
само порождение. Проверено тестом на настоящих процессах (Linux):
`cargo test -- --ignored kbsync_cancel_stops` — отмена за ~2,6 с при потомке,
который живёт 30 с и держит канал.

### 5. `main.rs` был переведён в CRLF

Прошлый коммит перевёл 22 015 строк из 22 251 в CRLF (вставленные правки остались с
LF), и сравнение с `fb73f2d` показывало 44 тысячи изменённых строк. Возвращён LF,
как в исходном файле, отдельным коммитом. **Важно для Windows-копии `D:\Signum-v1`:**
берите `main.rs` из репозитория целиком; если загрузить поверх локальный файл с CRLF,
сравнение снова станет нечитаемым.

## macOS: App Translocation и `sapsigner` (п. 7)

Подтверждено: `ensure_macos_sap_sidecar` копирует `v2/sapsigner` рядом с `v4/ipatool`,
то есть **внутрь Signum.app**. При запуске из «Загрузок»/образа .dmg пакет смонтирован
только для чтения, копирование падает (`sapsigner copy failed`), и вход отвечает «не
найден компонент подписи… переустановите Signum». То же будет, если приложение
поставил администратор, а работает обычный пользователь.

Сделано (`macos_go_engine_ready`): если рядом с Go-движком нет подписчика и в его
папку нельзя писать, движок вместе с подписчиком раскладывается в
`~/Library/Application Support/pw.appbank.signum/tools/<arch>/staged/v4/` и
запускается оттуда — так же, как живёт движок, приехавший обновлением. Когда папка
доступна для записи, поведение прежнее. Копируется содержимое файлов без
расширенных атрибутов, чтобы не унести карантин со скачанного .app. Движок v5 это не
затрагивает: подписчик у него внутри.

Логика раскладки покрыта тестом на Linux; **на настоящем маке не запускалось**.

## Установка IPA: `install -> upgrade` (п. 7 реализованных изменений)

Схема IPA_Downloader оставлена, уточнено, когда второй заход не делается: задание
отменено, телефон пропал (для этого свой повтор по Wi‑Fi), таймаут 360 с, сам
ideviceinstaller не запустился. Раньше отмена и таймаут тоже уходили в `upgrade` —
это ещё один запуск и до шести минут ожидания. Если не помогли оба, показывается
первый отказ; ошибка `upgrade` — только когда первая была общей фразой.

## После независимой проверки кода

Отдельный проход по диффу нашёл ещё несколько мест; исправлено:

- **Таймаут установки считался успехом при включённых «подробных ошибках».** Ветка
  «одни предупреждения про .sinf» стояла раньше проверки таймаута, а к тексту
  таймаута в этом режиме приклеен вывод процесса — у переподписанного IPA там всегда
  есть строки про `.sinf`. Ошибка старая (была и в `fb73f2d`), теперь таймаут в эту
  ветку не попадает.
- Код 2FA отправлен, а Apple ответила служебным кодом: вместо «введите тот же код
  ещё раз» (это звало сделать ещё одну попытку) показывается «Apple сейчас не
  принимает вход… подождите».
- `Install: Complete` + ненулевой код выхода ideviceinstaller теперь успех, а не
  повод заливать IPA заново через `upgrade`; `Complete` перед самым пределом 360 с
  тоже не превращается в таймаут.
- Опрос телефона после таймаута ограничен 20 с на попытку.
- Если KBSync породил сам движок (во время `list-versions` или загрузки),
  сохранённая копия сессии обновляется, чтобы blob не терялся при её восстановлении.

Известно и оставлено как есть:

- `ideviceinstaller` выходит с кодом 0, если телефон отключили посреди установки
  («Device removed») — Signum, как и раньше, считает это успехом. Требовать строку
  `Complete` при коде 0 не стал: неизвестно, все ли сборки инструмента её печатают,
  а ошибиться в эту сторону значит объявлять неудачей нормальные установки.
- Раскладка Go-движка на macOS копирует только файлы из его папки, без вложенных
  папок. Состав `apple-tools/macos-*/v4` в репозитории не лежит — проверьте на маке.

## Что осталось

1. Проверить на Windows с настоящим движком: вход, выход во время фонового прогрева
   (в `signum-report` должна появиться строка «прогрев KBSync остановлен через N с»),
   первую проверку каталога после входа (строка «каталог: прогрев KBSync … готов»).
2. Проверить установку локального IPA на настоящем iPhone: обычную, поверх уже
   стоящего приложения (`install -> upgrade`) и с отказом телефона.
3. Проверить на маке запуск из «Загрузок» без переноса в «Программы»: вход через V4,
   в логе — `engine folder is read-only (…); running …/staged/v4/ipatool`.
4. `warm-kbsync` в исходниках Signum нет (только в этом файле). Но движок из
   `signum-engine-build` знает лишь `warm-kbsync` и не знает `kbsync`: если он
   попадёт в сборку вместо релизного 1.2.0/1.3.0, прогрев будет получать
   «Unknown command». Сейчас в манифесте везде релизные сборки.
5. После этого — финальная сборка Windows-установщика (по отдельному решению).
