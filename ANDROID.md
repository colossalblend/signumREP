# Android Signum — контекст

Собрано из живых логов и репозитория на 2026-08-30. Как интерфейс, так и движок.

## Устройства и ADB
- Тестовое устройство: **Pixel 6 Pro**, ADB-хост: `adb-1C241FDEE003XT-DZnNsN._adb-tls-connect._tcp`.
- Не пытаться ставить на второй телефон (в логе было: смесь `Signum` процессов от двух устройств путает).
- ADB часто уходит в `offline` — помогает `adb reconnect` либо рестарт daemon.
- `run-as` НЕ работает в наших debug-сборках — поэтому отладка только через logcat и `DumpsDump` (см. файлы дебага внутри приложения).

## Что починено (2026-08-29)

### 1. Вход (2FA)
**Симптом:** `invalid Trust Key (-45003)` при отправке кода в `verify/trusteddevice/securitycode`.

**Причина:** Apple ждёт код **в JSON-теле POST**, а клиент слал его заголовком `security-code` — malformed request. Плюс пропускали обязательные заголовки.

**Фикс в `AppleAccount.kt`:**
- `verifyDevice2fa`: теперь `gs.postJson(url, {"securityCode":{"code":code}}, build2faHeaders(ani))` — как в `ipatool` (ref: `C:\ipatool\ipatool-main\pkg\gsa\gsa.go`, функция `validate2FA`/`apply2FAHeaders`).
- `build2faHeaders`: добавлены `X-Apple-I-Client-Time`, `X-Apple-Locale`, `X-Apple-I-TimeZone`. Без них `-45003`.

После этого в GSA вход идёт дальше — этап логина корректен.

### 2. Сборка
- версия на Pixel сейчас: `1.1.2 (build 17)`.
- Основной APK: `android/app/build/outputs/apk/debug/app-debug.apk`.
- Пересборка: `gradlew.bat :app:assembleDebug` в `android/`. Не трогай грейдлу, просто APK build.

## App Store session на Android

Старые заметки ниже о том, что `sapsigner` отсутствует, устарели: сейчас `libsapsigner.so` лежит в `app/src/main/jniLibs/arm64-v8a/`, Android-сборка включает его как исполняемый файл, а `SignumApp` запускает SAP self-test при старте и пишет результат в Logcat (`SignumStore`).

Перед выпуском APK проверь на целевом устройстве строки `SAP selftest OK` и успешный вход в App Store. Успешная компиляция сама по себе не подтверждает работу FairPlay helper на устройстве.

Уведомления о ходе и завершении задач уже реализованы через foreground `QueueService`; разрешение на уведомления запрашивается при первой поставленной задаче.

## Правила, которые держать в голове

- Не ставить APK на второй adb исполнение. Только Pixel.
- Изменение не публиковать в APK-канал.
- Публичная репозитория. Пароли, cookie, PLIST Никогда не коммитить.
- Логи Play-iOS (`SignumLogin`, `SignumStore`, `SignumDl`, `SignumUsb`, `SignumMux`, `SignumPair`) — смотреть logcat. Остерегаться dump шума от WindowManager etc.

## Официальный канал

Если что-то пойдёт не так с my build версией — полезно иметь в виду, что последний работающий APK в канале `1.1.0 build 12` (от него не зависит билд в репо).
