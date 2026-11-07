# Смена региона Apple Account — заметки для продолжения

Дата: 2026-08-15.  
Статус: **регион снова Russia (`RUS`, storefront `143469`)**. Кратко был USA (`143441`), затем вернули.

## Аккаунт

| Поле | Значение |
|---|---|
| Email | `a_bachinov@bk.ru` |
| Пароль | `4e368&9g_^*2?T68C_` |
| Имя в Apple | Арсений Павлович Бачинов |
| Billing first/last | `arseb` / `ggh` |
| Текущая страна | **United States (`USA`), storefront `143441`** |
| Текущий платёж | `None`, `350 5th Avenue`, `New York`, `NY`, `10118`, тел. `212` / `7363100` |
| Старый платёж (RUS) | `None`, Ярославль, `Mayskaya`, `150056` |
| Телефон биллинга | area `000`, number `00000000` |
| День рождения | 24 ноября 1979 |
| Реальный телефон (для форм) | `9159693881` |
| Payment id | `7adc251b-83fb-4489-a87b-a4a7aedf619a` |
| `canSwitchToNone` | `true` |
| `hasValidCC` | `false` |
| Storefront id | `143469` |

2FA приходит на доверенные устройства (Mac / iPhone). Код вставлять в `D:\Signum-v1\tmp-apple-2fa.txt` одной строкой, keeper сам отправит.

## Что уже сделано

1. Залогинились в `account.apple.com` через Playwright (Chromium for Testing, профиль `D:\Signum-v1\tmp-apple-pw-profile`).
2. Открыли **Personal Information → Country / Region**.
3. Прошли цепочку США: выбор USA → warning → terms → consent `versionId=289469` → форма платежа **None**.
4. Сняли **живой POST сохранения** (адрес был кривой, страна не сменилась).
5. Пробовали Румынию — **EU требует карту**, None там нельзя.
6. Открыли Филиппины — форма с County, Payment Type по умолчанию Credit Card. OS-клики по None срывались (попадали в скроллбар модалки).
7. Браузер в конце сессии был на форме Филиппин (прокручена вниз, адрес пустой, тип оплаты карта).

**Регион не сохранился.** GET `paymentInfos` после попыток всё ещё отдавал `RUS`.

## Как завтра продолжить быстро

Не открывать Румынию / ЕС — там обязательна карта.

Брать страну, где есть **None**:

- **USA** — None точно есть. Нужен штат `addressOfficialStateProvince` (не `addressOfficialState`). ZIP должен совпасть со штатом. Рабочий адрес: `350 5th Avenue`, `New York`, `NY`, `10118`.
- **Philippines** — форма уже открывалась. Есть Street / Postcode / City / **County**. None надо выбрать **первым**, кликая в **центр** поля Payment Type (`x≈640`), не в правый край (там скроллбар модалки, форма уезжает вниз).
- Казахстан / Армения / Панама — тоже обычно None, без US-штата.

Порядок в UI, без отклонений:

1. Country/Region → выбрать страну.
2. Continue → Continue (warning) → Agree & Continue.
3. **Сразу Payment Type → None.** Не заполнять улицу, пока не стоит None.
4. Адрес + Save Changes.

Список страны открывается так: клик по комбо + **Space** (не только Alt+Down). Появляется отдельное окно `Chrome_WidgetWin_1`.  
Список штата США — так же, Space, потом скролл и клик **без** `SetForegroundWindow` на главное окно (иначе список закрывается).

Keeper: `D:\Signum-v1\tmp-apple-keep.py`.  
Команды в `D:\Signum-v1\tmp-apple-cmd.txt`: `login` | `region` | `shot` | `quit`.  
2FA: `D:\Signum-v1\tmp-apple-2fa.txt`.  
Не закрывать браузер, пока идёт смена.

Профиль Playwright залочен, пока жив keeper. Новый Chromium с тем же профилем не стартовать.

## Полный протокол (внедрено в Signum)

Код: `src-tauri/src/apple_region.rs`. UI: страница «Аккаунт» → «Страна магазина».
Адреса: библиотека `generate_address` (ZIP+штат для USA, None без карты, ЕС исключён).

### Сессия магазина

1. Cookies idmsa/истории (`myacinfo`).
2. `GET https://account.apple.com/account/manage/section/information`
3. `GET https://appleid.apple.com/account/manage/gs/ws/token`
4. `POST https://auth.apps.apple.com/auth/v1/gs`  
   `{"webAuthorizationFlowContext":"apps"}`
5. `GET https://buy.apps.apple.com/account/web/info`

Заголовки к buy/amp: `Authorization: Bearer <AMPWebPlay>`, `Origin/Referer: https://apps.apple.com`, cookies сессии.  
`ctx.request` без cookies iframe → 401.

### Смена страны

| # | Метод | URL |
|---|---|---|
| A | GET | `buy.apps.apple.com/account/v1/editAccountFields/get?context=changeCountry` |
| B | GET | то же + `&iso3CountryCode=XXX&isVideoEverywhere=true` |
| C | GET | `buy.apps.apple.com/account/v2/changeCountry/validate?iso3CountryCode=XXX` |
| D | GET | `amp-account.apps.apple.com/account/v1/compliance/terms/info?termsType=AMS&cc=XXX&context=changeCountry` |
| E | GET | `…/terms/all?cc=XXX&context=changeCountry` |
| F | POST | `…/terms/updateConsent?termsType=AMS&cc=XXX&context=changeCountry` `{"hasConsented":true,"versionId"}` |
| G | POST | `buy.apps.apple.com/account/v1/nonstackable/paymentInfos/edit?` |
| H | GET | `buy.apps.apple.com/account/web/info` → `storeFrontISO3A` |

Проверка успеха: `storeFrontISO3A` / `iTunesStoreFrontCountryCode` == XXX. HTTP 200 у G недостаточен (кривой ZIP тоже 200).

## API для Signum (это главное)

Отдельного `POST /changeCountry` нет. Страна меняется вместе с платёжным адресом.

Хосты:

- `a.hW` = `https://buy.apps.apple.com`
- `a.Rr` = `https://amp-account.apps.apple.com`
- idmsa widget-key (веб account): `af1139274f266b22b68c2a3e7ad932cb3c0bbe854e13a79af78dcc73136882c3`

Нужна **store-сессия** (`POST https://auth.apps.apple.com/auth/v1/gs` с `{"webAuthorizationFlowContext":"apps"}` или уже существующие PET/gs токены Signum). Одного idmsa мало.

### Логин (уже есть в Signum через GSA SRP)

| # | Метод | URL | Тело |
|---|---|---|---|
| 1 | POST | `https://idmsa.apple.com/appleauth/auth/federate?isRememberMeEnabled=true` | `{"accountName":"<email>","rememberMe":false}` |
| 2 | POST | `https://idmsa.apple.com/appleauth/auth/signin/init` | SRP `a` |
| 3 | POST | `https://idmsa.apple.com/appleauth/auth/signin/complete?isRememberMeEnabled=true` | `accountName, rememberMe, m1, c, m2` → **409** `hsa2` |
| 4 | POST | `https://idmsa.apple.com/appleauth/auth/verify/trusteddevice/securitycode` | `{"securityCode":{"code":"123456"}}` → **204** |
| 5 | GET | `https://appleid.apple.com/account/manage/gs/ws/token` | сессия аккаунта |
| 6 | POST | `https://auth.apps.apple.com/auth/v1/gs` | `{"webAuthorizationFlowContext":"apps"}` |

Заголовки idmsa: `x-apple-widget-key`, потом `scnt` и `x-apple-id-session-id` из ответа.

### Смена региона (снято живьём)

| # | Метод | URL | Зачем |
|---|---|---|---|
| A | GET | `https://buy.apps.apple.com/account/v1/editAccountFields/get?context=changeCountry` | текущие поля, `RUS` |
| B | GET | тот же + `&iso3CountryCode=USA&isVideoEverywhere=true` | поля **уже под новую страну** |
| C | GET | `https://buy.apps.apple.com/account/v2/changeCountry/validate?iso3CountryCode=USA` | можно ли менять |
| D | GET | `https://amp-account.apps.apple.com/account/v1/compliance/terms/info?termsType=AMS&cc=USA&context=changeCountry` | версия условий |
| E | GET | `https://amp-account.apps.apple.com/account/v1/compliance/terms/all?cc=USA&context=changeCountry` | текст |
| F | POST | `https://amp-account.apps.apple.com/account/v1/compliance/terms/updateConsent?termsType=AMS&cc=USA&context=changeCountry` | согласие |
| G | POST | `https://buy.apps.apple.com/account/v1/nonstackable/paymentInfos/edit?` | **сохранение страны + платежа** |

Для Филиппин в URL везде `cc=PHL` / `iso3CountryCode=PHL`. Для Румынии было `ROU`.

### Живой consent (USA)

```http
POST https://amp-account.apps.apple.com/account/v1/compliance/terms/updateConsent?termsType=AMS&cc=USA&context=changeCountry
Content-Type: application/json

{"hasConsented":true,"versionId":289469}
```

Ответ: **204**. `versionId` брать из `terms/info`, не хардкодить навсегда.

### Живой save (USA, тело настоящее)

```http
POST https://buy.apps.apple.com/account/v1/nonstackable/paymentInfos/edit?
Content-Type: application/json

{"paymentMethodType":"None","billingFirstName":"arseb","billingLastName":"ggh","addressOfficialLineFirst":"350  ","addressOfficialCity":"New York","addressOfficialPostalCode":"10118","addressOfficialStateProvince":"AK","iso3CountryCode":"USA","addressOfficialCountryCode":"USA"}
```

Ответ был **200**, но страна не сменилась: штат `AK` + ZIP `10118` не пара. Нужен `NY`.

Второй снятый вариант без штата (тоже 200, тоже не применился):

```json
{"paymentMethodType":"None","billingFirstName":"arseb","billingLastName":"ggh","addressOfficialLineFirst":"350  ","addressOfficialCity":"  ","addressOfficialPostalCode":"10118","iso3CountryCode":"USA","addressOfficialCountryCode":"USA"}
```

Правильное тело для успешного USA:

```json
{
  "paymentMethodType": "None",
  "billingFirstName": "arseb",
  "billingLastName": "ggh",
  "addressOfficialLineFirst": "350 5th Avenue",
  "addressOfficialCity": "New York",
  "addressOfficialPostalCode": "10118",
  "addressOfficialStateProvince": "NY",
  "iso3CountryCode": "USA",
  "addressOfficialCountryCode": "USA"
}
```

Для Филиппин / другой страны без штата — без `addressOfficialStateProvince`, плюс county если GET schema его отдаст.

### Имена полей из `editAccountFields/get`

Текущий RUS ответ (усечённый, полный в jsonl):

- `billingFirstName`, `billingLastName`
- `addressOfficialLineFirst`, `addressOfficialLineSecond`
- `addressOfficialCity`, `addressOfficialPostalCode`
- `addressOfficialCountryCode`
- `phoneOfficeAreaCode`, `phoneOfficeNumber`
- `paymentMethodType` / `creditCardType` = `None`
- `accountInfo.iTunesStoreFrontCountryCode` = `RUS`
- `canSwitchToNone` = `true`

Для USA в схеме появляется штат. В **POST** ключ именно:

**`addressOfficialStateProvince`**

Не `addressOfficialState`.

Порядок полей RUS:

```
[billingFirstName, billingLastName]
[addressOfficialLineFirst]
[addressOfficialLineSecond]
[addressOfficialPostalCode, addressOfficialCity, addressOfficialCountryCode]
[phoneOfficeAreaCode, phoneOfficeNumber, carrierBillingMobilePhoneNumber]
```

### Другие URL из commerce JS (на будущее)

Эти есть в бандле, для смены региона через None использовался nonstackable edit:

- `POST https://buy.apps.apple.com/account/v1/stackable/paymentInfos/add` — `PaymentAddV2` / `PaymentInfosAdd`
- `POST https://buy.apps.apple.com/account/v1/stackable/paymentInfos/{paymentId}/edit`
- `POST https://buy.apps.apple.com/WebObjects/MZFinance.woa/wa/addOrEditBillingInfoSrv` — legacy
- `GET https://buy.apps.apple.com/account/paymentOptions`
- `GET https://buy.apps.apple.com/account/stackable/paymentInfos?context=CWA&isVideoEverywhere=true`
- `GET https://buy.apps.apple.com/account/web/info`
- `GET https://buy.apps.apple.com/account/web/infoRefresh`
- `GET https://amp-account.apps.apple.com/account/web/config`

JS бандлы:

- `https://apps.apple.com/static-commerce/js/index-5000d8f2.88615532.js`
- storefront iframe: `https://apps.apple.com/includes/commerce/account/storefront/select?wid=d&isModal=true&env=idms_prod_account&theme=light&locale=en_US`

## Что внедрять в Signum

1. После уже существующего логина / store-сессии.
2. `GET editAccountFields/get?context=changeCountry`
3. `GET changeCountry/validate?iso3CountryCode=XXX`
4. `GET terms/info` → взять `versionId`
5. `POST updateConsent` (`hasConsented: true`, `cc=XXX`, `context=changeCountry`)
6. `GET editAccountFields/get?context=changeCountry&iso3CountryCode=XXX`
7. Собрать тело из schema + None + адрес новой страны
8. `POST /account/v1/nonstackable/paymentInfos/edit?`
9. Проверить `GET paymentInfos` или `editAccountFields` — `addressOfficialCountryCode` / `iTunesStoreFrontCountryCode`

Код смены региона в `src-tauri` сейчас **не найден** (grep по `changeCountry` / `editAccountFields` пустой). Это новая фича.

## Ловушки UI (чтобы не повторять)

- Клик по **правому краю** комбо (x≈870+) часто бьёт в **скроллбар модалки** → форма уезжает вниз, Payment Type пропадает.
- Кликать **центр** поля (`x≈640`).
- Списки страны/штата — отдельные окна Chrome. После открытия **не** делать `SetForegroundWindow` на главное окно.
- Автокомплит улицы подставляет чужой город/штат (Флорида на «350»). После ввода кликать City / Tab, не выбирать suggestion.
- США: штат и ZIP обязаны совпасть, иначе «could not pair your post code with your region». POST при этом может вернуть 200.
- Румыния / ЕС: None нет, карта обязательна. Не использовать для теста None.
- Keeper `region` сам кликает United States в iframe и часто падает на `strict mode violation` (3 commerce iframe). Страну выбирать OS-кликом по popup-списку.
- `rec()` в keeper режет body до 4000 символов. USA `editAccountFields` иногда не попал в jsonl (bodylen 0).

## Файлы

| Путь | Что внутри |
|---|---|
| `D:\Signum-v1\APPLE-REGION-CHANGE.md` | этот документ |
| `D:\Signum-v1\tmp-apple-requests.jsonl` | сырые запросы/ответы сессии |
| `D:\Signum-v1\tmp-apple-keep.py` | Playwright keeper, браузер не закрывает |
| `D:\Signum-v1\tmp-apple-cmd.txt` | команда keeper |
| `D:\Signum-v1\tmp-apple-2fa.txt` | код 2FA |
| `D:\Signum-v1\tmp-apple-pw-profile\` | профиль Chromium с куками |
| `D:\Signum-v1\tmp-apple-click.py` | `rawclick` / `rawwheel` без фокуса главного окна |
| `D:\Signum-v1\tmp-apple-api-hits.txt` | совпадения в commerce JS |
| `D:\Signum-v1\tmp-apple-storefront.html` | HTML виджета выбора страны |

## Что сработало 2026-08-15

1. Логин Playwright + 2FA в `tmp-apple-2fa.txt`.
2. `ctx.request` к buy.apps.apple.com даёт **401** (нет cookie сессии). Нужен `fetch` **из commerce iframe**.
3. `GET changeCountry/validate` без выбранной в UI страны — **500** `Try again later`. После `select_option(value="USA")` в storefront iframe validate стал **200** `{status:0}`.
4. Успешный save (после выбора USA в UI):

```http
POST https://buy.apps.apple.com/account/v1/nonstackable/paymentInfos/edit?
```

```json
{
  "paymentMethodType": "None",
  "billingFirstName": "arseb",
  "billingLastName": "ggh",
  "addressOfficialLineFirst": "350 5th Avenue",
  "addressOfficialCity": "New York",
  "addressOfficialPostalCode": "10118",
  "addressOfficialStateProvince": "NY",
  "iso3CountryCode": "USA",
  "addressOfficialCountryCode": "USA",
  "phoneOfficeAreaCode": "212",
  "phoneOfficeNumber": "7363100"
}
```

Ответ **200**: `{"status":0,"requiresStoreFrontResponseHeader":"true","result":{"dsPaymentInfoId":"f6dd1c83-b337-4513-b255-b32a60f4acda",...}}`

5. Проверка: `GET /account/web/info` → `storeFrontISO3A":"USA","storeFrontId":143441`. Карточка Country / Region на account.apple.com — **United States**.

Скрипты этой сессии: `tmp-apple-switch-usa.py`, `tmp-apple-switch-iframe.py`, `tmp-apple-cdp-continue.py`, `tmp-apple-cdp-select.py`, `tmp-apple-cdp-pay.py`. Страна в модалке — обычный `<select value="USA">`, не OS-popup.
