import { Channel, invoke } from "@tauri-apps/api/core";
import type { StoreDeveloper, StoreSearchApp } from "../types";

const URL_PATTERN = /https?:\/\/[^\s)\]}>,]+/gi;
let detailedErrorsEnabled = false;

export function setDetailedErrors(enabled: boolean) {
  detailedErrorsEnabled = enabled;
}

                                                                                      
export function extractInvokeError(error: unknown): string {
  if (error == null) return "";
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message || String(error);
  if (typeof error === "object") {
    const obj = error as Record<string, unknown>;
    if (typeof obj.message === "string" && obj.message.trim()) return obj.message;
    if (typeof obj.error === "string" && obj.error.trim()) return obj.error;
    if (typeof obj.data === "string" && obj.data.trim()) return obj.data;
    try {
      return JSON.stringify(error);
    } catch {
      return String(error);
    }
  }
  return String(error);
}

function stripRustPaths(text: string): string {
  return text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line && !/\.rs:\d+/.test(line) && !/src-tauri|vendor[/\\]isideload/i.test(line))
    .join(" ")
    .replace(/\s{2,}/g, " ")
    .trim();
}

const APP_ID_LIMIT_MESSAGE =
  "На этом Apple ID кончился лимит подписей — у бесплатного их 10. Включите «Удалить плагины» или войдите другим Apple ID.";

function localizeSigningError(raw: string): string | null {
  const lower = raw.toLowerCase();
  if (
    raw.startsWith("SIGN_IPA_APP_ID_LIMIT")
    || lower.includes("not enough available app id")
    || (lower.includes("app id") && lower.includes("required") && lower.includes("available"))
  ) {
    return APP_ID_LIMIT_MESSAGE;
  }
  if (
    raw.startsWith("SIGN_IPA_NO_TEAM_DEVICE")
    || lower.includes("8220")
    || lower.includes("no devices from which to generate")
    || lower.includes("team has no devices")
    || (lower.includes("provisioning profile") && (lower.includes("no device") || lower.includes("failed to download")))
  ) {
    return "Для подписи Apple нужна хотя бы одна регистрация iPhone на этом Apple ID. Подключите iPhone кабелем, нажмите «Доверять» и повторите. Потом файл можно сохранять и без телефона.";
  }
  if (raw.startsWith("SIGN_IPA_CERT_FAILED") || raw.startsWith("SIGN_IPA_PROFILE_FAILED") || lower.includes("sign_ipa_cert_")) {
    const colon = raw.indexOf(":");
    const rest = colon > 0 ? stripRustPaths(raw.slice(colon + 1)) : "";
    if (/[А-Яа-яЁё]/.test(rest)) return rest;
    if (!rest) {
      return "Не удалось подписать файл. Повторите попытку. Если ошибка снова — выйдите и войдите в Apple ID.";
    }
    return "Не удалось получить сертификат для подписи. Повторите попытку или войдите другим Apple ID.";
  }
  return null;
}

/**
 * Достаёт путь из системного текста ошибки.
 *
 * Сообщения приходят вперемешку — от Windows, от ipatool, от наших команд, —
 * поэтому берём первое, что выглядит как путь, и не пытаемся разобрать
 * формат целиком.
 */
/**
 * Совет по отказу в доступе — разный для системных папок и для пользовательских.
 *
 * Различаем по пути, а не по тексту ошибки: Windows в обоих случаях говорит
 * одно и то же «access is denied», и без разбора пути человек получал совет
 * наугад. В Program Files и системных папках запись требует администратора;
 * в своей папке пользователя прав хватает всегда, значит мешает защита.
 */
function permissionAdvice(raw: string, path: string | null): string {
  if (!path) {
    return "Нет доступа к папке. Запустите Signum от имени администратора или разрешите ему запись в антивирусе.";
  }
  // Ищем по всему тексту ошибки, а не по вырезанному пути: «Program Files»
  // содержит пробел, а вырезанный путь на нём обрывается — именно так случай
  // «нужен администратор» и выдавал себя за антивирус.
  const lower = raw.toLowerCase();
  const systemArea =
    lower.includes(":\\program files") ||
    lower.includes(":\\programdata") ||
    lower.includes("c:\\windows\\");
  if (systemArea) {
    return `Нет прав на запись: ${path}

Signum установлен в системную папку, туда нельзя писать без администратора. Либо запустите Signum от имени администратора (правой кнопкой по ярлыку — «Запуск от имени администратора»), либо переустановите его в обычную папку пользователя — тогда права не понадобятся.`;
  }
  return `Нет доступа к файлу: ${path}

Папка пользовательская, прав у Signum достаточно — значит доступ блокирует защита. Отключите антивирус или добавьте в его исключения папку Signum. Проверьте ещё «Контролируемый доступ к папкам» в Защитнике Windows: Безопасность Windows → Защита от программ-шантажистов → Разрешить работу приложения.`;
}

function extractPath(raw: string): string | null {
  const windows = raw.match(/[A-Za-z]:\\[^\s"'<>|?*]+/);
  if (windows) return windows[0];
  const unix = raw.match(/\/(?:[\w.\-]+\/)+[\w.\-]+/);
  return unix ? unix[0] : null;
}

function localizeError(message: string): string {
  const raw = message.trim() || "Не удалось выполнить операцию";
  const signing = localizeSigningError(raw);
  if (signing) return signing;
  if (detailedErrorsEnabled) return stripRustPaths(raw) || raw;
  const lower = raw.toLowerCase();

                                                                                 
  if (raw.startsWith("SIGNUM_2FA_REQUIRED:")) return raw.replace(URL_PATTERN, "");
  if (raw.startsWith("SIGNUM_LOGIN_OPAQUE:")) {
    return raw.slice("SIGNUM_LOGIN_OPAQUE:".length).trim().replace(URL_PATTERN, "");
  }

                                                                             
  if (
    lower.includes("anisette")
    || lower.includes("x-apple-i-md")
    || (lower.includes("adi") && lower.includes("fail"))
  ) {
    return "Не удалось подготовить вход в Apple ID. Полностью закройте Signum и войдите снова.";
  }
  if (
    lower.includes("cookies.lock")
    || lower.includes("cannot acquire lock")
    || lower.includes("file locked")
    || lower.includes("lock-файл")
    || (lower.includes("занят") && (lower.includes("ipatool") || lower.includes("процесс") || lower.includes("действи")))
  ) {
    return "Signum сейчас занят. Подождите минуту и повторите попытку.";
  }
  if (
    lower.includes("cannot load cookies")
    || (lower.includes("panic") && (lower.includes("cookie") || lower.includes("goroutine")))
    || lower.includes("invalid character '#'")
    || (lower.includes("looking for beginning of value") && lower.includes("cookie"))
  ) {
    return "Сохранённый вход повреждён. Нажмите «Войти снова».";
  }
  if (lower.includes("ipatool") && (lower.includes("not logged in") || lower.includes("не сохрани"))) {
    return "Вход в Apple ID не сохранился. Войдите снова — Apple может запросить код.";
  }
  if (lower.includes("keychain") || (lower.includes("ключ") && lower.includes("ipatool"))) {
    return "Сбой сохранённого входа. Выйдите из аккаунта и войдите снова. При необходимости проверьте ключ в «Настройках».";
  }

                                                                                         
  if (
    /[А-Яа-яЁё]/.test(raw) &&
    (lower.includes("apple id") ||
      lower.includes("iphone") ||
      lower.includes("сессия") ||
      lower.includes("не удалось войти") ||
      lower.includes("неверный") ||
      lower.includes("заблокировал") ||
      lower.includes("отключён") ||
      lower.includes("отключен") ||
      lower.includes("icloud") ||
      lower.includes("itunes"))
  ) {
    return raw
      .replace(URL_PATTERN, "")
      .replace(/\b(?:anisette(?:\.exe)?|ipatool(?:\.exe)?|idevice\w*|lock-файл|cookies\.lock|keychain)\b/gi, "")
      .replace(/\s{2,}/g, " ")
      .replace(/\s+([.,;:!?])/g, "$1")
      .trim();
  }

                                                                                
  if (raw.startsWith("SIGN_IPA_FILE_REQUIRED") || lower.includes("sign_ipa_file_required")) {
    return "Выберите существующий файл IPA.";
  }
  if (raw.startsWith("SIGN_IPA_AUTH_REQUIRED") || lower.includes("sign_ipa_auth_required")) {
    return "Войдите в Apple ID. Для подписи нужен сохранённый пароль — войдите снова на странице «Аккаунт».";
  }
  if (
    raw.startsWith("SIGN_IPA_DEVICE_REQUIRED")
    || raw.startsWith("SIGN_IPA_DEVICE_DISCONNECTED")
    || lower.includes("sign_ipa_device_")
  ) {
    return "Подключите и выберите iPhone, разблокируйте его и повторите попытку.";
  }
  if (raw.startsWith("SIGN_IPA_BUNDLE_INVALID") || lower.includes("sign_ipa_bundle_invalid")) {
    const colon = raw.indexOf(":");
    const rest = colon > 0 ? raw.slice(colon + 1).trim() : "";
    return rest || "В приложении есть расширение с чужим идентификатором. Включите «Удалить плагины / extensions» и повторите.";
  }
  if (raw.startsWith("SIGN_IPA_NETWORK") || lower.includes("sign_ipa_network")) {
    return "Не удалось связаться с серверами Apple. Проверьте интернет или VPN и повторите попытку.";
  }
  if (
    raw.startsWith("SIGN_IPA_DEV_LOGIN_FAILED")
    || lower.includes("sign_ipa_dev_login")
  ) {
                                                                    
    const colon = raw.indexOf(":");
    if (colon > 0) {
      const rest = raw.slice(colon + 1).trim();
      if (/[А-Яа-яЁё]/.test(rest)) return rest;
    }
    return "Не удалось войти для подписи. Выйдите и войдите снова в «Аккаунт», затем повторите.";
  }
  if (raw.startsWith("SIGN_IPA_TEAM_NONE") || lower.includes("sign_ipa_team_none")) {
    return "У этого Apple ID нет developer team.";
  }
  if (
    raw.startsWith("SIGN_IPA_CERT_FAILED")
    || raw.startsWith("SIGN_IPA_PROFILE_FAILED")
    || lower.includes("sign_ipa_cert_")
    || lower.includes("sign_ipa_profile_")
  ) {
    return "Apple не выдала разрешение на подпись. Повторите попытку или войдите другим Apple ID.";
  }
  if (raw.startsWith("SIGN_IPA_ZSIGN_FAILED") || lower.includes("sign_ipa_zsign")) {
    return "Не удалось собрать подписанный IPA. Повторите подпись. Если ошибка снова — полностью закройте Signum и войдите в Apple ID заново.";
  }
  if (lower.includes("plist parse") || lower.includes("unexpectedendofeventstream")) {
    return "Apple вернул повреждённый ответ при входе для подписи. Проверьте интернет/VPN, выйдите и войдите в «Аккаунт», затем повторите.";
  }
  if (raw.startsWith("SIGN_IPA_INSTALL_FAILED") || lower.includes("sign_ipa_install")) {
    return "IPA подписан, но установка на iPhone не удалась. Проверьте кабель и доверие к устройству.";
  }
  if (raw.startsWith("SIGN_IPA_CANCELLED") || lower.includes("sign_ipa_cancelled")) {
    return "Операция отменена.";
  }

  if (
    lower.includes("error sending request") ||
    lower.includes("failed to fetch") ||
    lower.includes("networkerror") ||
    lower.includes("network error") ||
    lower.includes("dns error") ||
    lower.includes("connection refused") ||
    lower.includes("connection reset") ||
    lower.includes("connect error") ||
    lower.includes("could not resolve host")
  ) {
    return "Не удалось подключиться к сервису. Проверьте интернет и повторите попытку.";
  }
  if (lower.includes("timed out") || lower.includes("timeout") || lower.includes("deadline exceeded")) {
    return "Операция заняла слишком много времени. Проверьте подключение и повторите попытку.";
  }
  if (/\b(?:http\s*)?(?:status\s*)?429\b/i.test(raw)) {
    return "Слишком много запросов. Подождите немного и повторите попытку.";
  }
  if (/\b(?:http\s*)?(?:status\s*)?(?:500|502|503|504)\b/i.test(raw)) {
    return "Сервис временно недоступен. Повторите попытку позже.";
  }
  if (/\b(?:http\s*)?(?:status\s*)?(?:401|403)\b/i.test(raw)) {
    return "Сервис отклонил запрос. Проверьте авторизацию или обновите Signum.";
  }
  if (/\b(?:http\s*)?(?:status\s*)?404\b/i.test(raw)) {
    return "Запрошенные данные не найдены или временно недоступны.";
  }
  if (lower.includes("invalid json") || lower.includes("decode error") || lower.includes("expected value at line")) {
    return "Сервис вернул некорректные данные. Повторите попытку позже.";
  }
  if (lower.includes("no device found") || lower.includes("device not found") || lower.includes("no device connected")) {
    return "iPhone не найден. Подключите устройство кабелем, разблокируйте его и повторите попытку.";
  }
  if (
    lower.includes("trust dialog")
    || lower.includes("please accept the trust")
    || lower.includes("accept the trust")
    || lower.includes("user denied the trust")
  ) {
    return "На iPhone нажмите «Доверять» (при необходимости введите код-пароль). Телефон должен быть разблокирован. Затем снова нажмите «Переподключить доверие».";
  }
  if (lower.includes("not paired") || lower.includes("pairing") || lower.includes("lockdownd") || lower.includes("idevicepair")) {
    return "Нет доверия между компьютером и iPhone. Разблокируйте телефон, нажмите «Доверять» на экране iPhone и снова «Переподключить доверие».";
  }
  if (lower.includes("permission denied") || lower.includes("access is denied") || lower.includes("os error 5")) {
    // Путь обязателен: без него «нет доступа к файлу» невозможно
    // диагностировать — ни нам по скриншоту, ни человеку у себя.
    return permissionAdvice(raw, extractPath(raw));
  }
  if (lower.includes("no such file") || lower.includes("file not found") || lower.includes("cannot find the file") || lower.includes("os error 2")) {
    return "Нужный файл не найден. Проверьте путь и повторите попытку.";
  }
  if (lower.includes("incorrect password") || lower.includes("invalid password") || lower.includes("incorrect account")) {
    return "Неверный Apple ID или пароль.";
  }
  if (lower.includes("session expired") || lower.includes("session has expired") || lower.includes("not authenticated")) {
    return "Сессия Apple ID устарела. Войдите в аккаунт снова.";
  }
  if (
    lower.includes("password token is expired")
    || lower.includes("passwordtoken is expired")
    || lower.includes("password token expired")
    || (lower.includes("token is expired") && lower.includes("password"))
  ) {
    return "Сессия Apple ID для App Store устарела. Откройте «Аккаунт», выйдите и войдите снова (может понадобиться код 2FA).";
  }
  if (
    lower.includes("terms and conditions")
    || lower.includes("media services")
    || lower.includes("мультимедийн")
    || lower.includes("положение и условия")
    || (lower.includes("условия") && lower.includes("измен"))
  ) {
    return "Apple просит подтвердить условия магазина. Подождите минуту и повторите установку.";
  }
  if (lower.includes("account not in store") || lower.includes("accountnotinstore") || lower.includes("storefront ()") || lower.includes("store front ()")) {
    return "Apple ID ещё не настроен для покупок в App Store. Откройте App Store на iPhone, войдите в «Медиаматериалы и покупки», примите условия и установите любое бесплатное приложение. Затем заново войдите в Apple ID в Signum.";
  }
  if (lower.includes("purchase history") || lower.includes("not purchased") || lower.includes("license not found")) {
    return "Приложение не найдено в покупках этого Apple ID.";
  }
  if (lower.includes("applicationverificationfailed") || lower.includes("verification failed")) {
    return "iPhone отклонил приложение при проверке. Переподключите устройство и повторите установку.";
  }
  if (lower.includes("installation failed") || lower.includes("install failed")) {
    return "Не удалось установить приложение на iPhone. Проверьте подключение и свободное место.";
  }
  if (lower.includes("cancelled") || lower.includes("canceled")) {
    return "Операция отменена.";
  }
  if (lower.includes("завершился с кодом") || lower.includes("process exited with code")) {
    return "Вспомогательный компонент завершился с ошибкой. Проверьте подключение, Apple ID и повторите попытку.";
  }

  const cleaned = raw
    .replace(URL_PATTERN, "")
    .replace(/\s*\(os error \d+\)/gi, "")
    .replace(/\s{2,}/g, " ")
    .replace(/\s+([:;,.])/g, "$1")
    .trim();
                                                                                
  if (
    lower.includes("missing field")
    || lower.includes("unknown field")
    || lower.includes("duplicate field")
    || lower.includes("invalid type")
    || lower.includes("invalid args")
    || lower.includes("failed to deserialize")
  ) {
    return "Сбой передачи данных во вход. Полностью закройте Signum, откройте снова и повторите вход.";
  }
  if (!cleaned) return "Не удалось выполнить операцию. Повторите попытку.";
  if (/[А-Яа-яЁё]/.test(cleaned)) {
    const russianLines = cleaned
      .split(/\r?\n/)
      .filter(line => /[А-Яа-яЁё]/.test(line))
      .map(line => line.replace(/:\s*[A-Za-z][\s\S]*$/, ".").trim())
      .filter(Boolean);
    return russianLines.join("\n") || "Не удалось выполнить операцию. Повторите попытку.";
  }
  return "Не удалось выполнить операцию. Повторите попытку.";
}

let lastAutoKey = "";
let lastAutoAt = 0;

function isTechnicalFault(text: string): boolean {
  const lower = (text || "").toLowerCase();
  return (
    lower.includes("discarding cookies")
    || lower.includes("cookiejar")
    || lower.includes("illegal base64")
    || lower.includes("cannot unmarshal")
    || lower.includes("failed to get account")
    || lower.includes("failed to get item")
    || lower.includes("cannot load cookies")
  );
}

function shouldSkipAutoError(message: string): boolean {
  const text = message.trim();
  if (!text) return true;
  if (isTechnicalFault(text)) return false;
  const lower = text.toLowerCase();
  if (lower.includes("отменено") || lower.includes("cancelled")) return true;
  if (lower.includes("signum_2fa_required")) return true;
  if (lower.includes("введите код") || (lower.includes("нужен код") && lower.includes("iphone"))) return true;
  if (lower.includes("не удалось сменить страну")) return true;
  if (lower.includes("нет доступных версий")) return true;
  if (lower.includes("нет подтвержд") && lower.includes("верси")) return true;
  if (
    lower.includes("не найдено в app store")
    || lower.includes("удалили из магазина")
    || (lower.includes("недоступно") && lower.includes("регион"))
  ) return true;
  return false;
}

/** Fold timestamps/ids so repeats of one fault share a key. ipatool stamps
 *  every error with the wall clock, which used to defeat this guard. */
function autoErrorKey(text: string): string {
  return text.replace(/\d{4}-\d{2}-\d{2}T[\d:.+\-Z]+/gi, "#").replace(/\d+/g, "#");
}

function reportAutoError(message: string, raw: string) {
  const technical = isTechnicalFault(message) || isTechnicalFault(raw);
  if (!technical && shouldSkipAutoError(message)) return;
  const now = Date.now();
  // The backend classifies message + raw into a stable English code, so send
  // both as-is instead of guessing here which one is the interesting half.
  const key = autoErrorKey(message);
  if (key === lastAutoKey && now - lastAutoAt < 60_000) return;
  lastAutoKey = key;
  lastAutoAt = now;
  void invoke("report_auto_error", {
    source: "ui",
    message: message.slice(0, 800),
    detail: raw && raw !== message ? raw.slice(0, 4000) : null,
  }).catch(() => undefined);
}

export async function command<T>(name: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(name, args);
  } catch (error) {
    const raw = extractInvokeError(error) || "Не удалось выполнить команду";
    throw new Error(localizeError(raw));
  }
}

export interface AppStoreVersionResult {
  version: string;
  externalVersionId: string;
  isLatest: boolean;
  alreadyOwned: boolean;
  minIos?: string;
}

export interface AppStoreVersionsPageResult {
  versions: AppStoreVersionResult[];
  hasMore: boolean;
  total: number;
  offset: number;
  /** Owned, but absent from this Apple ID's storefront: history cannot be
   *  listed even though the latest build still downloads. */
  regionBlocked?: boolean;
}

export function fetchAppStoreVersions(
  adamId: string,
  opts?: { offset?: number; refresh?: boolean },
): Promise<AppStoreVersionsPageResult> {
  return command<AppStoreVersionsPageResult>("fetch_app_store_versions", {
    input: {
      adamId,
      offset: opts?.offset ?? 0,
      limit: 12,
      refresh: opts?.refresh ?? false,
    },
  });
}

export function resolveAppStoreVersionsIos(
  adamId: string,
  externalVersionIds: string[],
  onReady: (values: Record<string, string>) => void,
): Promise<Record<string, string>> {
  const onUpdate = new Channel<Record<string, string>>();
  onUpdate.onmessage = onReady;
  return command("resolve_app_store_versions_ios", {
    input: { adamId, externalVersionIds }, onUpdate,
  });
}

export function resolveAppStoreVersionsLabels(
  adamId: string,
  externalVersionIds: string[],
  onReady: (values: Record<string, string>) => void,
): Promise<Record<string, string>> {
  const onUpdate = new Channel<Record<string, string>>();
  onUpdate.onmessage = onReady;
  return command("resolve_app_store_versions_labels", {
    input: { adamId, externalVersionIds }, onUpdate,
  });
}

export async function namedAppStoreVersions(adamId: string, versions: AppStoreVersionResult[]) {
  const named = (version: AppStoreVersionResult) => !!version.version.trim()
    && version.version !== version.externalVersionId && !/^\d{8,}$/.test(version.version);
  const missing = versions.filter(version => !named(version)).map(version => version.externalVersionId);
  const labels = missing.length
    ? await resolveAppStoreVersionsLabels(adamId, missing, () => {}) : {};
  return versions.map(version => labels[version.externalVersionId]
    ? { ...version, version: labels[version.externalVersionId] } : version).filter(named);
}

export function searchStoreApps(query: string): Promise<StoreSearchApp[]> {
  return command<StoreSearchApp[]>("search_store_apps", { input: { query } });
}

/** Разработчики, чьё имя похоже на запрос. */
export function searchStoreDevelopers(query: string): Promise<StoreDeveloper[]> {
  return command<StoreDeveloper[]>("search_store_developers", { input: { query } });
}

/** Все приложения одного разработчика. */
export function developerApps(artistId: string): Promise<StoreSearchApp[]> {
  return command<StoreSearchApp[]>("developer_apps", { input: { artistId } });
}

/** Текст ошибки для показа без отправки в диагностику (для рендера списков). */
export function displayError(error: unknown): string {
  return localizeError(extractInvokeError(error) || "Не удалось выполнить операцию");
}

export function errorMessage(error: unknown): string {
  const raw = extractInvokeError(error) || "Не удалось выполнить операцию";
  const message = localizeError(raw);
  reportAutoError(message, raw);
  return message;
}

/**
 * Записать в диагностический лог, что человек нажал.
 *
 * Без этого лог читался как поток запросов без причины: непонятно, сам ли
 * человек полез за версиями или это фоновая работа. Ошибки глотаем — запись в
 * лог не имеет права мешать тому, ради чего кнопку нажали.
 */
export function logUi(action: string, detail?: string): void {
  void command("log_ui_action", { action, detail: detail ?? null }).catch(() => {});
}
