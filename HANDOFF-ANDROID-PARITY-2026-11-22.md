# Handoff: Android ↔ ПК паритет, ipatool 1.3.2, каталог (2026-11-22)

Ветка: `claude/clever-allen-06dtww` (включает всё из `claude/trusting-einstein-p4nq35`).
Базовая точка до работы: `de205a4`.

## Задача пользователя

1. Портировать ipatool на Android полностью 1:1 и синхронизировать функции ПК → Android.
2. «Результаты App Store» — карточками (как на ПК).
3. Карточки каталога и результаты поиска AppMagic + iTunes с **одинаковым App Store ID** — объединять в одну карточку (без дублей).
4. Исправить ошибки, сделать дизайн приятнее (ПК и Android).

Работа была разделена на 4 агента. Статус ниже.

## 1. Движок App Store на Android = ipatool 1.3.2 — ГОТОВО (не проверено на устройстве)

Коммиты: `e397468`, `786ca35`, `9034585`, `a209326`. Подробности — `android/ENGINE.md` (раздел про 1.3.2).

- Новый `data/apple/StoreProtocol.kt` (чистая логика) + `StoreProtocolTest.kt` (17 тестов): коды/сообщения ошибок ipatool,
  тела buyProduct / redownload / updateProduct, разбор bag.xml, storefront→страна, патч `iTunesMetadata.plist`, md5.
- `AppStoreClient.kt`:
  - покупка через `MZFinance.woa/wa/buyProduct` (2059 → GAME pricing, 5002 = уже куплено, платные отклоняются);
  - каскад скачивания: volumeStoreDownloadProduct → **redownloadProduct (новое)** → updateProduct (строгая проверка);
  - «нужна лицензия» → покупка + повтор; 2034/2042/1008/«Sign In» → один тихий релогин;
  - md5 + повторная загрузка, songDownloadDone, метаданные как у iTunes;
  - логин: до 3 попыток на 5xx, стоп на 429, DSID из `x-dsid`;
  - новые методы: `accountInfo()`, `revoke()`, `searchAccount()`, `versionMetadata()`.
- Публичный API для `SignumViewModel` не менялся.

Сознательные отличия от ПК: нет kbsync/X-Apple-AMD (нужен эмулятор CoreADI) — шаг 1 каскада на старом passwordToken;
нет `.pkg`; нет iTunesArtwork в IPA (нужно в `IpaTweaks`); list-purchases — старый порт 1.1.8.

**Проверить на Pixel:** вход + 2FA, покупку (новый путь MZFinance), скачивание (особенно redownloadProduct), список версий, мин. iOS.

## 2. Каталог и поиск на Android как на ПК — ГОТОВО (не скомпилировано)

Коммиты: `6474f96`, `b91770c` (+ восстановления после откатов).

- Новый `data/catalog/StoreSearch.kt` (порт `src-tauri/src/store_search.rs`) + `StoreSearchTest.kt` (6 тестов, проходят на JVM).
- AppMagic + iTunes (ru+us) → одна запись на ID; совпадение с каталогом → одна карточка каталога, обогащённая данными магазина.
- Раздел «Результаты App Store» карточками со всеми состояниями (поиск, ошибка, скелетоны, пусто, «всё уже в каталоге»).
- Поиск разработчиков, окно разработчика с отметками и «Скачать отмеченные», сортировка (RU-алфавит, версия iOS численно),
  фильтр совместимости выключается без iPhone.
- Не перенесено (нет движка на Android): покупка на нескольких Apple ID, «только купить», временная смена страны, постраничное «Загрузить ещё» версий.

## 3. ПК — ГОТОВО

Коммиты: `c620fad`, `2da09d1`, `711ca52`, `f6d30dd`.

- Дедуп AppMagic/iTunes по ID (строка/число), матч с каталогом по ID, затем по bundle ID; Rust принимает числовые ID (+2 теста).
- Исправлены: необработанные ошибки установки/экспорта и двойная постановка, гонки «Другие версии» и страницы разработчика,
  «Повторить» в баннере обновления, двойное окно 2FA при подписи.
- Единый стиль карточек, hover/press, фокус-кольца, reduced-motion.
- Проверено: `pnpm exec tsc --noEmit` чисто, `pnpm build` ок, `cargo test` 152/152 (с локальными стабами из `src-tauri/ci-stubs/`).
- Не проверено визуально (приложение не запускалось).

## 4. Остальные экраны Android + дизайн — В РАБОТЕ / может быть не доделано

Коммиты на момент записи: `a0fb590` (тема, формы, общие виджеты), `2e95811`/`776497e` (очередь: пауза, повтор, порядок,
отмена ожидающих; экран установок), `8287420` (восстановление темы/виджетов/очереди после отката), `12e7745`
(восстановление вне главного потока, история по страницам).

Что оставалось по плану (проверить `git log` — могло быть доделано):
- сверить `src/pages/{Account,Devices,IpaSigning,Settings,Uninstall,About}.tsx` и модалки
  (`ActivationModal`, `BankAltInstallModal`, `AppTransfer`, `UpdateBanners`, `IcloudSetupGate`, `Signing2faGate`,
  `ClientOutdatedGate`) с экранами `ui/screens/*Screen.kt` и перенести недостающее;
- баги Android (краши, null, утечки корутин);
- полировка нижней навигации.

## ⚠️ Важно для следующего, кто продолжит

1. **Android ни разу не собирался.** Локально нет Android SDK; CI (`.github/workflows/build-android.yml`) на GitHub не
   стартует из-за ошибки оплаты аккаунта («recent account payments have failed»). Первым делом:
   ```bash
   cd android && ./gradlew :app:compileDebugKotlin :app:testDebugUnitTest
   ```
   Ожидаемы мелкие ошибки компиляции в `CatalogScreen.kt` / `SignumViewModel.kt` (проверялись только глазами).
2. **Git-ловушка с worktree.** Если несколько worktree держат одну и ту же ветку, после чужого коммита у остальных старые
   файлы выглядят как staged-изменения, и обычный `git commit` молча откатывает чужую работу (так случилось 3 раза).
   Работать в своей локальной ветке и пушить `git push origin HEAD:claude/clever-allen-06dtww`; перед коммитом смотреть
   `git diff --cached --stat`.
3. После сборки проверить, что на месте: `data/apple/StoreProtocol.kt`, `data/catalog/StoreSearch.kt`,
   `ui/theme/Theme.kt`, `ui/components/Widgets.kt`, `JobQueueTest.kt`.
4. В основной копии есть `stash@{0}` — это устаревшее дерево до работы агентов, не реальные изменения; можно удалить.
