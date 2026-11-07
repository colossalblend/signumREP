import { useSyncExternalStore } from "react";
import type {
  AppStoreVersion,
  CatalogApp,
  CatalogInstallOffer,
  InstallJob,
  IpaInspect,
  PurchaseHistoryApp,
  StoreSearchApp,
} from "../types";

/**
 * First-launch walkthrough state.
 *
 * The tour drives the real pages instead of showing its own slides, so the
 * pages need to know when to render demo rows. Demo data exists only while the
 * tour runs: it keeps the walkthrough from touching Apple (no login, no
 * purchase lookups, no downloads) and disappears the moment the tour ends.
 */

const STORAGE_KEY = "signum_onboarding_v2";

export function hasSeenOnboarding(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === "1";
  } catch {
    return true;
  }
}

export function markOnboardingSeen() {
  try {
    localStorage.setItem(STORAGE_KEY, "1");
  } catch {
    /* private mode — just don't persist */
  }
}

export function resetOnboarding() {
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch {
    /* ignore */
  }
}

/**
 * Предложение пройти обучение на главной. Показывается тем, кто обучение не
 * закончил, и исчезает навсегда после первого закрытия или после прохождения.
 */
const OFFER_KEY = "signum_tour_offer_v1";
const DONE_KEY = "signum_tour_done_v1";

export function tourOfferHidden(): boolean {
  try {
    return localStorage.getItem(OFFER_KEY) === "closed" || localStorage.getItem(DONE_KEY) === "1";
  } catch {
    return true;
  }
}

export function hideTourOffer() {
  try {
    localStorage.setItem(OFFER_KEY, "closed");
  } catch {
    /* private mode — just don't persist */
  }
}

export function markTourCompleted() {
  try {
    localStorage.setItem(DONE_KEY, "1");
  } catch {
    /* ignore */
  }
}

let active = false;
const listeners = new Set<() => void>();

function emit() {
  for (const fn of listeners) fn();
}

function subscribe(fn: () => void) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function setTourActive(value: boolean) {
  if (active === value) return;
  active = value;
  if (!value) signal = { seq: signal.seq + 1, name: "" };
  emit();
}

/**
 * One-way commands from the walkthrough to a page: the tour asks Catalog to
 * open its real install sheet with the demo offer, asks Find to show demo
 * results, and so on. Driving the real components keeps the tour honest — the
 * user is looking at the same UI they will use, not a replica.
 */
export type TourSignal = { seq: number; name: string };

let signal: TourSignal = { seq: 0, name: "" };

export function sendTourSignal(name: string) {
  signal = { seq: signal.seq + 1, name };
  emit();
}

export function useTourSignal(): TourSignal {
  return useSyncExternalStore(subscribe, () => signal, () => signal);
}

export function isTourActive() {
  return active;
}

/** True while the walkthrough is on screen — pages use it to add demo rows. */
export function useTourDemo(): boolean {
  return useSyncExternalStore(subscribe, () => active, () => false);
}

/* ------------------------------------------------------------------ */
/* Demo content. Ids are namespaced so nothing can collide with a real */
/* catalog entry, and every label says «Тест» so a screenshot of the   */
/* tour can never be mistaken for a real install.                      */
/* ------------------------------------------------------------------ */

export const TOUR_APP_ID = "signum-tour-demo-app";

/** Демо-«подключённый» iPhone: без него проверка совместимости остаётся пустой. */
export const TOUR_DEVICE_IOS = "17.4";

export const tourCatalogApp: CatalogApp = {
  id: TOUR_APP_ID,
  title: "Тест приложение",
  bundle_id: "pw.appbank.test",
  app_store_id: "0",
  version: "3.4.0",
  min_ios: "14.0",
  in_store: true,
  color: "#5b6cf0",
  group: "app",
} as CatalogApp;

export const tourInstallOffer: CatalogInstallOffer = {
  app_id: TOUR_APP_ID,
  app_title: "Тест приложение",
  title: "Тест приложение",
  description: "Демонстрационное приложение Signum — показывает, как выглядит установка.",
  version: "3.4.0",
  bundle_id: "pw.appbank.test",
  app_store_id: "0",
  min_ios: "14.0",
  in_store: true,
  source: "app_store",
  message: "Версия доступна в App Store и будет привязана к Apple ID перед установкой.",
  show_account_versions: true,
  device_ios: TOUR_DEVICE_IOS,
  device_compatible: true,
} as CatalogInstallOffer;

export const tourVersions: AppStoreVersion[] = [
  { version: "3.4.0", externalVersionId: "918273645", isLatest: true, alreadyOwned: true, minIos: "16.0" },
  { version: "3.2.1", externalVersionId: "918001122", isLatest: false, alreadyOwned: true, minIos: "15.0" },
  { version: "2.9.0", externalVersionId: "917553301", isLatest: false, alreadyOwned: true, minIos: "14.0" },
  { version: "916402277", externalVersionId: "916402277", isLatest: false, alreadyOwned: false, minIos: "18.2" },
];

export const tourSearchHits: StoreSearchApp[] = [
  { name: "Тест приложение", publisher: "Signum Demo Ltd.", iconUrl: "", appStoreId: "0" },
  {
    name: "Тест приложение Lite",
    publisher: "Signum Demo Ltd.",
    iconUrl: "",
    appStoreId: "1",
    removedFromStore: true,
  },
];

export const tourHistoryRows: PurchaseHistoryApp[] = [
  { adamId: "tour-1", name: "Тест приложение", artist: "Signum Demo Ltd.", bundleId: "pw.appbank.test", date: "2026-08-14" },
  { adamId: "tour-2", name: "Тест Банк Онлайн", artist: "Signum Demo Ltd.", bundleId: "pw.appbank.testbank", date: "2026-06-02" },
  { adamId: "tour-3", name: "Тест Почта", artist: "Signum Demo Ltd.", bundleId: "pw.appbank.testmail", date: "2025-11-19" },
];

export const tourIpaDecrypted: IpaInspect = {
  kind: "decrypted",
  title: "Расшифрованный",
  app_name: "Тест приложение",
  version: "3.4.0",
  explanation:
    "Файл нужно подписать вашим Apple ID. После этого его можно поставить на iPhone или сохранить.",
  needs_sign: true,
} as IpaInspect;

export const tourIpaEncrypted: IpaInspect = {
  kind: "encrypted",
  title: "Зашифрованный",
  app_name: "Тест приложение из App Store",
  version: "3.4.0",
  explanation:
    "Этот файл из App Store. Его можно сразу поставить на iPhone — дополнительная подпись не нужна.",
  needs_sign: false,
} as IpaInspect;

export const TOUR_EMAIL = "demo@icloud.com";
export const TOUR_CODE = "418032";

export const tourInstallJob: InstallJob = {
  id: "signum-tour-demo-job",
  kind: "install",
  title: "Тест приложение",
  subtitle: "v3.4.0 · сборка 918273645",
  status: "running",
  phase: "download",
  percent: 0,
  retryable: false,
} as InstallJob;
