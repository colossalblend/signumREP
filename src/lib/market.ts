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
// Заменяется ответом сервера, когда он появится.

function seller(id: string, name: string, price: number, rating: number, sales: number, storefront = "RU"): MarketSeller {
  return { id, name, price, rating, sales, storefront };
}

export const TEST_MARKET_APPS: MarketApp[] = [
  {
    id: "vk", title: "ВКонтакте", bundle_id: "com.vk.vkclient", app_store_id: "564177498",
    developer: "VK", category: "social", color: "#0077ff", min_ios: "15.0",
    sellers: [
      seller("vk-a", "AppBank", 149, 4.9, 1280),
      seller("vk-b", "iStore TR", 129, 4.7, 540, "TR"),
      seller("vk-c", "FastInstall", 199, 4.8, 210),
    ],
  },
  {
    id: "vk-video", title: "VK Видео", bundle_id: "com.vk.vkvideo", app_store_id: "1446471760",
    developer: "VK", category: "media", color: "#0077ff", min_ios: "15.0",
    sellers: [seller("vkv-a", "AppBank", 149, 4.9, 410)],
  },
  {
    id: "tbank", title: "Т-Банк", bundle_id: "com.tcsbank.ib", app_store_id: "488573820",
    developer: "Tbank", category: "finance", color: "#ffdd2d", min_ios: "15.0",
    sellers: [
      seller("tb-a", "AppBank", 199, 4.9, 2300),
      seller("tb-b", "SafeInstall", 179, 4.6, 870),
    ],
  },
  {
    id: "sber", title: "СберБанк Онлайн", bundle_id: "ru.sberbankmobile", app_store_id: "492224193",
    developer: "Sberbank", category: "finance", color: "#21a038", min_ios: "15.0",
    sellers: [seller("sb-a", "AppBank", 199, 4.9, 1950)],
  },
  {
    id: "happ", title: "Happ - Proxy Utility", bundle_id: "com.happproxy", app_store_id: "6504287215",
    developer: "Happ", category: "utilities", color: "#5b6cf0", min_ios: "15.0",
    sellers: [
      seller("hp-a", "AppBank", 99, 4.8, 320),
      seller("hp-b", "iStore TR", 89, 4.5, 180, "TR"),
    ],
  },
  {
    id: "max", title: "MAX", bundle_id: "ru.oneme.app", app_store_id: "6480873435",
    developer: "VK", category: "messenger", color: "#2d7bff", min_ios: "15.0",
    sellers: [seller("max-a", "AppBank", 129, 4.7, 140)],
  },
];

export function searchMarket(apps: MarketApp[], query: string, category: MarketCategory | "all"): MarketApp[] {
  const q = query.trim().toLowerCase();
  return apps.filter(app => {
    if (category !== "all" && app.category !== category) return false;
    if (!q) return true;
    return (
      app.title.toLowerCase().includes(q) ||
      app.developer.toLowerCase().includes(q) ||
      app.bundle_id.toLowerCase().includes(q) ||
      app.app_store_id.includes(q)
    );
  });
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
