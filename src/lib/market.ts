// Маркет Signum — витрина, где продавцы выставляют установку приложений со
// своего Apple ID за свою цену. Клиент выбирает приложение, Signum проверяет
// iPhone (iOS 17.4+), проводит оплату (ЮKassa) и переносит данные FairPlay на
// устройство, как в проекте Verni.
//
// ВСЁ ТЕСТОВО. Серверная часть ещё не готова, поэтому каталог и продавцы здесь
// — временные данные, а оплата, формирование FairPlay и передача на iPhone
// имитируются с задержками. Проверка «подключён ли iPhone и iOS 17.4+» и
// проверка установленного приложения используют настоящие команды Signum, если
// они доступны. Когда появится сервер, нужно заменить только функции в блоке
// «Заглушки сервера» ниже — разметка и состояния менять не придётся.

import { command } from "./tauri";
import type { DeviceInfo, InstalledApp } from "../types";

/** Минимальная версия iOS, с которой работает перенос FairPlay. */
export const MARKET_MIN_IOS = "17.4";

export interface MarketSeller {
  id: string;
  /** Отображаемое имя продавца (аккаунт Apple ID скрыт до оплаты). */
  name: string;
  /** Цена установки в рублях. */
  price: number;
  /** Рейтинг 0–5 и число успешных установок — для доверия. */
  rating: number;
  sales: number;
  /** Регион витрины Apple ID продавца (TR, RU, US…). */
  storefront: string;
}

export interface MarketApp {
  id: string;
  title: string;
  /** Bundle ID — по нему идёт поиск и проверка установки на iPhone. */
  bundle_id: string;
  /** Числовой App Store ID — тоже доступен в поиске. */
  app_store_id: string;
  developer: string;
  category: MarketCategory;
  color: string;
  icon_url?: string;
  min_ios: string;
  sellers: MarketSeller[];
}

export type MarketCategory =
  | "social"
  | "finance"
  | "messenger"
  | "media"
  | "utilities";

export const MARKET_CATEGORIES: { id: MarketCategory; label: string }[] = [
  { id: "social", label: "Соцсети" },
  { id: "messenger", label: "Мессенджеры" },
  { id: "finance", label: "Финансы" },
  { id: "media", label: "Медиа" },
  { id: "utilities", label: "Утилиты" },
];

export function categoryLabel(id: MarketCategory): string {
  return MARKET_CATEGORIES.find(c => c.id === id)?.label ?? id;
}

export function formatPrice(rub: number): string {
  return new Intl.NumberFormat("ru-RU", { style: "currency", currency: "RUB", maximumFractionDigits: 0 }).format(rub);
}

/** Минимальная цена среди продавцов — для карточки. */
export function minPrice(app: MarketApp): number {
  return app.sellers.reduce((min, s) => Math.min(min, s.price), Infinity);
}

// ───────────────────────── Тестовый каталог ─────────────────────────
// Заменяется ответом сервера, когда он появится. У продавца один id на все
// приложения — это его аккаунт в Маркете; по нему работает фильтр.

const SELLERS = {
  appbank: { name: "AppBank", rating: 4.9, storefront: "RU" },
  istore: { name: "iStore TR", rating: 4.7, storefront: "TR" },
  fast: { name: "FastInstall", rating: 4.8, storefront: "RU" },
  safe: { name: "SafeInstall", rating: 4.6, storefront: "RU" },
  usa: { name: "US Apps", rating: 4.5, storefront: "US" },
} as const;
type SellerKey = keyof typeof SELLERS;

function seller(key: SellerKey, price: number, sales: number): MarketSeller {
  return { id: key, ...SELLERS[key], price, sales };
}

export const TEST_MARKET_APPS: MarketApp[] = [
  {
    id: "vk", title: "ВКонтакте", bundle_id: "com.vk.vkclient", app_store_id: "564177498",
    developer: "VK", category: "social", color: "#0077ff", min_ios: "15.0",
    sellers: [seller("appbank", 149, 1280), seller("istore", 129, 540), seller("fast", 199, 210)],
  },
  {
    id: "vk-video", title: "VK Видео", bundle_id: "com.vk.vkvideo", app_store_id: "1446471760",
    developer: "VK", category: "media", color: "#0077ff", min_ios: "15.0",
    sellers: [seller("appbank", 149, 410)],
  },
  {
    id: "tbank", title: "Т-Банк", bundle_id: "com.tcsbank.ib", app_store_id: "488573820",
    developer: "Tbank", category: "finance", color: "#ffdd2d", min_ios: "15.0",
    sellers: [seller("appbank", 199, 2300), seller("safe", 179, 870)],
  },
  {
    id: "sber", title: "СберБанк Онлайн", bundle_id: "ru.sberbankmobile", app_store_id: "492224193",
    developer: "Sberbank", category: "finance", color: "#21a038", min_ios: "15.0",
    sellers: [seller("appbank", 199, 1950), seller("safe", 189, 640), seller("usa", 219, 95)],
  },
  {
    id: "alfa", title: "Альфа-Банк", bundle_id: "ru.alfabank.mobile.iphone", app_store_id: "1453826808",
    developer: "Alfa-Bank", category: "finance", color: "#ef3124", min_ios: "15.0",
    sellers: [seller("appbank", 199, 1120), seller("fast", 209, 300)],
  },
  {
    id: "vtb", title: "ВТБ Онлайн", bundle_id: "ru.vtb24.mobilebanking", app_store_id: "1466457427",
    developer: "VTB", category: "finance", color: "#0a2973", min_ios: "15.0",
    sellers: [seller("safe", 179, 520)],
  },
  {
    id: "happ", title: "Happ - Proxy Utility", bundle_id: "com.happproxy", app_store_id: "6504287215",
    developer: "Happ", category: "utilities", color: "#5b6cf0", min_ios: "15.0",
    sellers: [seller("appbank", 99, 320), seller("istore", 89, 180), seller("usa", 79, 60)],
  },
  {
    id: "max", title: "MAX", bundle_id: "ru.oneme.app", app_store_id: "6480873435",
    developer: "VK", category: "messenger", color: "#2d7bff", min_ios: "15.0",
    sellers: [seller("appbank", 129, 140), seller("fast", 119, 70)],
  },
  {
    id: "vk-music", title: "VK Музыка", bundle_id: "com.uma.musicvk", app_store_id: "1534578419",
    developer: "VK", category: "media", color: "#ff3347", min_ios: "15.0",
    sellers: [seller("istore", 119, 230)],
  },
];

/** Продавец в целом по Маркету — для ленты фильтра. */
export interface MarketSellerSummary {
  id: string;
  name: string;
  rating: number;
  storefront: string;
  apps: number;
  sales: number;
  minPrice: number;
}

export function marketSellers(apps: MarketApp[]): MarketSellerSummary[] {
  const byId = new Map<string, MarketSellerSummary>();
  for (const app of apps) {
    for (const s of app.sellers) {
      const cur = byId.get(s.id);
      if (cur) {
        cur.apps += 1;
        cur.sales += s.sales;
        cur.minPrice = Math.min(cur.minPrice, s.price);
      } else {
        byId.set(s.id, { id: s.id, name: s.name, rating: s.rating, storefront: s.storefront, apps: 1, sales: s.sales, minPrice: s.price });
      }
    }
  }
  return [...byId.values()].sort((a, b) => b.sales - a.sales);
}

export type MarketSort = "popular" | "price_asc" | "price_desc" | "rating";

export const MARKET_SORTS: { id: MarketSort; label: string }[] = [
  { id: "popular", label: "Популярные" },
  { id: "price_asc", label: "Сначала дешевле" },
  { id: "price_desc", label: "Сначала дороже" },
  { id: "rating", label: "По рейтингу продавцов" },
];

export interface MarketFilter {
  query: string;
  category: MarketCategory | "all";
  /** Пусто — все продавцы. */
  sellerIds: string[];
  /** Верхняя граница цены в рублях; 0 — без ограничения. */
  maxPrice: number;
  sort: MarketSort;
}

export const EMPTY_FILTER: MarketFilter = { query: "", category: "all", sellerIds: [], maxPrice: 0, sort: "popular" };

/** Продавцы приложения с учётом фильтра по продавцам и цене. */
export function matchingSellers(app: MarketApp, filter: Pick<MarketFilter, "sellerIds" | "maxPrice">): MarketSeller[] {
  return app.sellers.filter(s =>
    (filter.sellerIds.length === 0 || filter.sellerIds.includes(s.id))
    && (filter.maxPrice === 0 || s.price <= filter.maxPrice));
}

function totalSales(app: MarketApp): number {
  return app.sellers.reduce((sum, s) => sum + s.sales, 0);
}

export function filterMarket(apps: MarketApp[], filter: MarketFilter): MarketApp[] {
  const q = filter.query.trim().toLowerCase();
  const list = apps.filter(app => {
    if (filter.category !== "all" && app.category !== filter.category) return false;
    if (matchingSellers(app, filter).length === 0) return false;
    if (!q) return true;
    return (
      app.title.toLowerCase().includes(q) ||
      app.developer.toLowerCase().includes(q) ||
      app.bundle_id.toLowerCase().includes(q) ||
      app.app_store_id.includes(q)
    );
  });
  const price = (app: MarketApp) => Math.min(...matchingSellers(app, filter).map(s => s.price));
  const rating = (app: MarketApp) => Math.max(...matchingSellers(app, filter).map(s => s.rating));
  switch (filter.sort) {
    case "price_asc": return list.sort((a, b) => price(a) - price(b));
    case "price_desc": return list.sort((a, b) => price(b) - price(a));
    case "rating": return list.sort((a, b) => rating(b) - rating(a));
    default: return list.sort((a, b) => totalSales(b) - totalSales(a));
  }
}

/** Цвет аватара продавца — стабильный по имени. */
export function sellerHue(name: string): number {
  let h = 0;
  for (const ch of name) h = (h * 31 + ch.charCodeAt(0)) % 360;
  return h;
}

export function pluralRu(n: number, one: string, few: string, many: string): string {
  const m10 = n % 10, m100 = n % 100;
  if (m10 === 1 && m100 !== 11) return one;
  if (m10 >= 2 && m10 <= 4 && (m100 < 12 || m100 > 14)) return few;
  return many;
}

// ───────────────────────── Поток покупки и установки ─────────────────────────

export type PurchaseStage =
  | "select_seller"   // выбор продавца (если их больше одного)
  | "connect"         // подключение iPhone
  | "check_ios"       // проверка iOS 17.4+
  | "ios_too_low"     // iOS ниже требуемой
  | "confirm"         // подтверждение и оплата
  | "paying"          // оплата через ЮKassa
  | "forming"         // формирование данных FairPlay
  | "transferring"    // передача на iPhone
  | "verifying"       // проверка установки на iPhone
  | "installed"       // готово
  | "failed";         // ошибка

export interface DeviceSnapshot {
  udid: string;
  name: string;
  model?: string;
  ios: string;
}

export function iosAtLeast(version: string, min = MARKET_MIN_IOS): boolean {
  const parse = (v: string) => (v.split(/\D+/).filter(Boolean).map(n => parseInt(n, 10) || 0).concat([0, 0, 0]));
  const [a, b, c] = parse(version);
  const [x, y, z] = parse(min);
  if (a !== x) return a > x;
  if (b !== y) return b > y;
  return c >= z;
}

/** Найти подключённый iPhone настоящей командой Signum. */
export async function findConnectedDevice(): Promise<DeviceSnapshot | null> {
  try {
    const devices = await command<DeviceInfo[]>("list_connected_devices");
    const device = devices.find(d => (d.product_version ?? "").length > 0) ?? devices[0];
    if (!device) return null;
    return {
      udid: device.udid,
      name: device.device_name || "iPhone",
      model: device.product_type,
      ios: device.product_version || "",
    };
  } catch {
    return null;
  }
}

/**
 * Проверить, установлено ли приложение на подключённом iPhone. Настоящая
 * команда Signum; при недоступности (нет сервера/устройства в тесте) бросает —
 * вызывающий решает, считать ли это тестовым успехом.
 */
export async function isAppInstalled(bundleId: string): Promise<boolean> {
  const apps = await command<InstalledApp[]>("list_installed_apps");
  return apps.some(a => a.bundle_id === bundleId && !a.is_placeholder);
}

// ───────────────────────── Заглушки сервера (ТЕСТ) ─────────────────────────
// Эти три функции имитируют работу сервера-обработчика. Когда появится
// серверная часть и оплата через ЮKassa, заменить тела на реальные вызовы;
// сигнатуры и этапы менять не нужно.

const delay = (ms: number) => new Promise<void>(resolve => setTimeout(resolve, ms));

export interface TestFlags {
  /** true = имитировать подключённый iPhone, когда настоящего нет. */
  simulateDevice?: boolean;
  /** Версия iOS у имитируемого iPhone. */
  simulatedIos?: string;
  /** true = считать установку успешной, не обращаясь к устройству. */
  simulateInstalled?: boolean;
}

/** Тестовая оплата через ЮKassa. Реальная будет возвращать статус платежа. */
export async function testPay(seller: MarketSeller): Promise<void> {
  await delay(1400);
  void seller;
}

/** Тестовое формирование данных FairPlay на сервере владельца. */
export async function testFormFairplay(app: MarketApp, device: DeviceSnapshot): Promise<void> {
  await delay(1600);
  void app; void device;
}

/** Тестовая передача данных FairPlay на подключённый iPhone. */
export async function testTransfer(app: MarketApp, device: DeviceSnapshot): Promise<void> {
  await delay(1600);
  void app; void device;
}
