import { ArrowLeft, ArrowUpRight, Check, ChevronRight, Download, LoaderCircle, RotateCcw, Search, ShoppingBag, Smartphone, Store, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import AppCard from "./AppCard";
import Checkbox from "./Checkbox";
import { isTwoFactorError } from "../lib/auth";
import { formatMinIos, formatVersionWithIos } from "../lib/format";
import {
  command, developerApps, errorMessage, fetchAppStoreVersions, logUi,
  searchStoreApps, searchStoreDevelopers, resolveAppStoreVersionsIos, namedAppStoreVersions,
} from "../lib/tauri";
import type {
  AppStoreVersion,
  CatalogApp,
  CatalogInstallOffer,
  StoreDeveloper,
  StoreRegionPlan,
  StoreSearchApp,
  AppleAccountInfo,
  AuthStatus,
} from "../types";

/** «1 приложение», «2 приложения», «5 приложений». */
/// Склонение слова «разработчик» по числу.
function pluralDevs(count: number): string {
    const tail = count % 100;
    if (tail >= 11 && tail <= 14) return "разработчиков";
    switch (count % 10) {
        case 1: return "разработчик";
        case 2:
        case 3:
        case 4: return "разработчика";
        default: return "разработчиков";
    }
}

function pluralApps(count: number): string {
  const tens = count % 100;
  if (tens >= 11 && tens <= 14) return "приложений";
  switch (count % 10) {
    case 1: return "приложение";
    case 2:
    case 3:
    case 4: return "приложения";
    default: return "приложений";
  }
}

import { tourSearchHits, useTourDemo } from "../lib/tourStore";

/** App Store ID приходит и строкой, и числом; сравниваем только цифры. */
function normalizeStoreId(value: unknown): string {
  const text = value == null ? "" : String(value).trim();
  return /^\d+$/.test(text) ? text.replace(/^0+(?=\d)/, "") : "";
}

function normalizeBundleId(value: unknown): string {
  return value == null ? "" : String(value).trim().toLowerCase();
}

const VERSIONS_PAGE = 12;

function versionMatchesSearch(version: string, externalVersionId: string, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  const v = (version || "").trim().toLowerCase();
  const id = (externalVersionId || "").trim().toLowerCase();
  const qDigits = q.replace(/\s/g, "");
  if (v.includes(q) || id.includes(q) || id.includes(qDigits)) return true;
  if (v.replace(/^v/, "").includes(q.replace(/^v/, ""))) return true;
  return false;
}

function ResultIcon({ url, name, large = false, small = false }: {
  url: string; name: string; large?: boolean; small?: boolean;
}) {
  const [broken, setBroken] = useState(false);
  // Мелкий размер нужен в строке разработчика: там значков три подряд, и
  // обычные сорок восемь пикселей съедали бы всю строку.
  const size = large
    ? "h-16 w-16 rounded-2xl drop-shadow-icon-lg"
    : small
      ? "h-8 w-8 rounded-lg"
      : "h-12 w-12 rounded-xl";
  const letter = large ? "text-2xl" : small ? "text-xs" : "text-lg";
  if (!url.trim() || broken) {
    return (
      <div className={`grid ${size} shrink-0 place-items-center bg-brand-gradient font-black ${letter}`}>
        {name.slice(0, 1)}
      </div>
    );
  }
  return (
    <img
      src={url}
      alt=""
      className={`${size} shrink-0 object-cover`}
      referrerPolicy="no-referrer"
      draggable={false}
      onError={() => setBroken(true)}
    />
  );
}

function StoreHitCard({ hit, onClick, dataTour }: {
  hit: StoreSearchApp;
  onClick: () => void;
  dataTour?: string;
}) {
  const [broken, setBroken] = useState(false);
  const badge = hit.foundOnAccount
    ? { className: "border-emerald-400/20 bg-emerald-400/10 text-emerald-300", icon: <RotateCcw size={11} />, label: "Есть на Apple ID" }
    : hit.removedFromStore
      ? { className: "border-amber-400/20 bg-amber-400/10 text-amber-300", icon: <Store size={11} />, label: "Нет в App Store" }
      : null;

  return (
    <button
      type="button"
      data-tour={dataTour}
      onClick={onClick}
      className="panel group relative min-h-[270px] overflow-hidden p-5 text-left transition duration-300 hover:-translate-y-1 hover:border-white/[0.13] hover:bg-white/[0.055]"
    >
      <div className="pointer-events-none absolute -right-12 -top-12 h-40 w-40 rounded-full bg-brand-500 opacity-20 blur-3xl transition group-hover:opacity-35" />
      <div className="flex items-start justify-between gap-3">
        <div className="relative h-[86px] w-[86px]">
          <div className="absolute inset-2 translate-y-3 rounded-[24px] bg-brand-500 opacity-30 blur-xl" />
          <div className="absolute inset-1 translate-x-2 translate-y-2 rounded-[22px] bg-black/35" />
          {hit.iconUrl.trim() && !broken ? (
            <img
              src={hit.iconUrl}
              alt=""
              draggable={false}
              referrerPolicy="no-referrer"
              onError={() => setBroken(true)}
              className="relative h-[78px] w-[78px] rounded-[20px] object-cover drop-shadow-icon-lg ring-1 ring-white/20"
            />
          ) : (
            <div className="relative grid h-[78px] w-[78px] place-items-center rounded-[20px] bg-brand-gradient text-3xl font-black drop-shadow-icon-lg">{hit.name.slice(0, 1)}</div>
          )}
        </div>
        {badge && (
          <span className={`inline-flex items-center gap-1.5 rounded-lg border px-2.5 py-1 text-[10px] font-bold uppercase tracking-wider ${badge.className}`}>
            {badge.icon}{badge.label}
          </span>
        )}
      </div>
      <div className="mt-6">
        <h3 className="line-clamp-2 text-lg font-bold tracking-tight text-white">{hit.name}</h3>
        {hit.publisher.trim() && <p className="mt-1 truncate text-xs text-slate-500">{hit.publisher.trim()}</p>}
      </div>
      <span className="absolute bottom-5 right-5 grid h-8 w-8 place-items-center rounded-lg bg-white/[0.055] text-brand-300 transition group-hover:bg-brand-500 group-hover:text-white"><ArrowUpRight size={15} /></span>
    </button>
  );
}

/** Заглушка карточки на время первого поиска — той же высоты, что настоящая. */
function CardSkeleton() {
  return (
    <div aria-hidden className="panel min-h-[270px] animate-pulse p-5">
      <div className="h-[78px] w-[78px] rounded-[20px] bg-white/[0.06]" />
      <div className="mt-8 h-4 w-3/4 rounded bg-white/[0.06]" />
      <div className="mt-2 h-3 w-1/2 rounded bg-white/[0.04]" />
    </div>
  );
}

function formatAppStoreVersionLabel(version: string, externalVersionId: string): string {
  const v = (version || "").trim();
  const id = (externalVersionId || "").trim();
  if (!v) return id ? "сборка" : "—";
  if (id && v === id) return "сборка";
  if (/^\d{8,}$/.test(v)) return "сборка";
  return v.startsWith("v") || v.startsWith("V") ? v : `v${v}`;
}

type StoreSearchProps = {
  /** Текст из общей строки поиска Каталога. Владеет им страница, а не этот блок. */
  query: string;
  /** Обучение показывает демо-результаты вместо запроса к Apple. */
  tourShowHits: boolean;
  /** Весь каталог Signum: результат App Store с тем же ID объединяется с карточкой каталога. */
  catalogApps: CatalogApp[];
  /** Карточки каталога, уже показанные выше по этому запросу, — их дубли из App Store скрываем. */
  shownCatalogIds: ReadonlySet<string>;
  onOpenCatalogApp: (app: CatalogApp) => void;
  /** Избранное каталога: объединённая карточка ведёт себя так же, как выше. */
  favoriteIds?: readonly string[];
  onToggleFavorite?: (app: CatalogApp) => void;
  /** Сообщаем странице, идёт ли сетевой поиск, — для спиннера в строке ввода. */
  onSearchingChange?: (searching: boolean) => void;
};

/**
 * App Store-часть Каталога: поиск по AppMagic + iTunes, разработчики и окно
 * установки найденного. У неё своё состояние и свои номера запросов, поэтому
 * она никогда не пересекается с локальным фильтром каталога Signum: каталог
 * фильтруется мгновенно в памяти, а сюда запрос уходит только когда открыта
 * вкладка «App Store» и введено от двух символов.
 */
export default function StoreSearch({
  query, tourShowHits, catalogApps, shownCatalogIds, onOpenCatalogApp, favoriteIds, onToggleFavorite, onSearchingChange,
}: StoreSearchProps) {
  const navigate = useNavigate();
  const [storeHits, setStoreHits] = useState<StoreSearchApp[]>([]);
  const [storeSearching, setStoreSearching] = useState(false);
  const [storeError, setStoreError] = useState("");

  const [offer, setOffer] = useState<CatalogInstallOffer | null>(null);
  const [appStoreVersions, setAppStoreVersions] = useState<AppStoreVersion[]>([]);
  const [versionsLoading, setVersionsLoading] = useState(false);
  const [versionsLoadingMore, setVersionsLoadingMore] = useState(false);
  const [iosLoadingIds, setIosLoadingIds] = useState<Set<string>>(new Set());
  const [versionsError, setVersionsError] = useState("");
  const [versionsNeedsAuth, setVersionsNeedsAuth] = useState(false);
  const [versionsHasMore, setVersionsHasMore] = useState(false);
  const [versionsRegionBlocked, setVersionsRegionBlocked] = useState(false);
  const [versionsTotal, setVersionsTotal] = useState(0);
  const [chosenVersion, setChosenVersion] = useState<AppStoreVersion | null>(null);
  const [versionsVisibleCount, setVersionsVisibleCount] = useState(VERSIONS_PAGE);
  const [versionsSearch, setVersionsSearch] = useState("");
  const [busy, setBusy] = useState(false);
  const [purchasing, setPurchasing] = useState(false);
  const [purchaseNote, setPurchaseNote] = useState("");
  const [actionError, setActionError] = useState("");
  const [regionPlan, setRegionPlan] = useState<StoreRegionPlan | null>(null);
  const [regionPlanBusy, setRegionPlanBusy] = useState(false);

  // Поиск по разработчику. Отдельный режим, а не примесь к обычному поиску:
  // искать студию и искать приложение — разные задачи, и смешанный список
  // («то приложения, то студии») читался бы как каша.
  // Режима больше нет: по одному вводу ищем и приложения, и разработчиков.
  // Выбирать вручную было лишней работой для человека — он и так не знает
  // заранее, найдётся ли студия с таким именем.
  // Отмеченные для пакетной загрузки приложения разработчика.
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [batchBusy, setBatchBusy] = useState(false);
  const [devHits, setDevHits] = useState<StoreDeveloper[]>([]);
  const [devSearching, setDevSearching] = useState(false);
  const [devError, setDevError] = useState("");
  const [openDev, setOpenDev] = useState<StoreDeveloper | null>(null);
  const [devApps, setDevApps] = useState<StoreSearchApp[]>([]);
  const [devAppsLoading, setDevAppsLoading] = useState(false);
  const [devAppsError, setDevAppsError] = useState("");
  const [purchaseDialog, setPurchaseDialog] = useState(false);
  const [purchaseAccounts, setPurchaseAccounts] = useState<AppleAccountInfo[]>([]);
  const [purchaseAccountEmails, setPurchaseAccountEmails] = useState<string[]>([]);
  const [purchaseBusy, setPurchaseBusy] = useState(false);
  const [purchaseError, setPurchaseError] = useState("");
  const [purchaseResult, setPurchaseResult] = useState("");

  const storeRequestRef = useRef(0);
  const devRequestRef = useRef(0);
  const devAppsRequestRef = useRef(0);
  const versionsRequestRef = useRef(0);
  const versionsOffsetRef = useRef(0);

  useEffect(() => {
    const text = query.trim();
    if (text.length < 2) {
      storeRequestRef.current += 1;
      setStoreHits([]);
      setStoreError("");
      setStoreSearching(false);
      return;
    }
    const request = ++storeRequestRef.current;
    setStoreSearching(true);
    const timer = window.setTimeout(() => {
      void searchStoreApps(text)
        .then(hits => {
          if (request !== storeRequestRef.current) return;
          setStoreHits(hits);
          setStoreError("");
        })
        .catch(error => {
          if (request !== storeRequestRef.current) return;
          setStoreHits([]);
          setStoreError(errorMessage(error));
        })
        .finally(() => {
          if (request === storeRequestRef.current) setStoreSearching(false);
        });
    }, 280);
    return () => window.clearTimeout(timer);
  }, [query]);

  useEffect(() => {
    if (!offer) setRegionPlan(null);
  }, [offer]);

  // Решаем за пользователя: покупать, переезжать или ничего не делать.
  // Непустой список версий означает, что приложение уже в покупках, даже если
  // магазин отвечает «недоступно в вашей стране».
  useEffect(() => {
    const appStoreId = offer?.app_store_id?.trim();
    if (!appStoreId || versionsLoading) return;
    let cancelled = false;
    setRegionPlanBusy(true);
    void command<StoreRegionPlan>("plan_store_region_for_app", {
      input: { appStoreId, ownedVersions: appStoreVersions.length > 0 },
    })
      .then(plan => { if (!cancelled) setRegionPlan(plan); })
      .catch(() => { if (!cancelled) setRegionPlan(null); })
      .finally(() => { if (!cancelled) setRegionPlanBusy(false); });
    return () => { cancelled = true; };
  }, [offer?.app_store_id, versionsLoading, appStoreVersions.length]);

  const filteredVersions = useMemo(
    () => appStoreVersions.filter(item => versionMatchesSearch(item.version, item.externalVersionId, versionsSearch)),
    [appStoreVersions, versionsSearch],
  );

  async function loadVersions(selected: CatalogInstallOffer, refresh = false) {
    const request = ++versionsRequestRef.current;
    if (!selected.app_store_id?.trim()) return;
    setVersionsLoading(true);
    setVersionsLoadingMore(false);
    setIosLoadingIds(new Set());
    setVersionsError("");
    setVersionsNeedsAuth(false);
    setAppStoreVersions([]);
    setChosenVersion(null);
    setVersionsHasMore(false);
    setVersionsRegionBlocked(false);
    setVersionsTotal(0);
    setVersionsVisibleCount(VERSIONS_PAGE);
    setVersionsSearch("");
    try {
      const page = await fetchAppStoreVersions(selected.app_store_id, { refresh });
      if (request !== versionsRequestRef.current) return;
      setVersionsRegionBlocked(!!page.regionBlocked);
      const versions = await namedAppStoreVersions(selected.app_store_id, page.versions || []);
      if (request !== versionsRequestRef.current) return;
      versionsOffsetRef.current = page.offset + (page.versions || []).length;
      setAppStoreVersions(versions);
      setVersionsHasMore(!!page.hasMore);
      setVersionsTotal(page.total || versions.length);
      setChosenVersion(versions.find(item => item.isLatest) ?? versions[0] ?? null);
      void loadVersionIos(selected.app_store_id, versions, request);
    } catch (error) {
      if (request === versionsRequestRef.current) {
        const message = errorMessage(error);
        setVersionsError(message);
        setVersionsNeedsAuth(isTwoFactorError(message) || /сначала войдите|сессия.*(?:истек|не сохран)|apple id/i.test(message));
      }
    } finally {
      if (request === versionsRequestRef.current) setVersionsLoading(false);
    }
  }

  async function loadVersionIos(adamId: string, versions: AppStoreVersion[], request: number) {
    const ids = versions.filter(version => !version.minIos).map(version => version.externalVersionId);
    if (!ids.length) return;
    setIosLoadingIds(previous => new Set([...previous, ...ids]));
    const apply = (values: Record<string, string>) => {
      if (request !== versionsRequestRef.current) return;
      const update = (version: AppStoreVersion) => values[version.externalVersionId]
        ? { ...version, minIos: values[version.externalVersionId] } : version;
      setAppStoreVersions(previous => previous.map(update));
      setChosenVersion(previous => previous ? update(previous) : null);
      setIosLoadingIds(previous => new Set([...previous].filter(id => !values[id])));
    };
    try {
      apply(await resolveAppStoreVersionsIos(adamId, ids, apply));
    } catch {
      // Compatibility is optional; a failed lookup must not block downloading.
    } finally {
      if (request === versionsRequestRef.current) {
        setIosLoadingIds(previous => new Set([...previous].filter(id => !ids.includes(id))));
      }
    }
  }

  async function loadMoreVersions(selected: CatalogInstallOffer) {
    if (versionsLoading || versionsLoadingMore || !versionsHasMore || !selected.app_store_id?.trim()) return;
    const request = versionsRequestRef.current;
    setVersionsLoadingMore(true);
    try {
      const page = await fetchAppStoreVersions(selected.app_store_id, {
        offset: versionsOffsetRef.current,
      });
      if (request !== versionsRequestRef.current) return;
      const incoming = await namedAppStoreVersions(selected.app_store_id, page.versions || []);
      if (request !== versionsRequestRef.current) return;
      versionsOffsetRef.current = page.offset + (page.versions || []).length;
      void loadVersionIos(selected.app_store_id, incoming, request);
      setAppStoreVersions(prev => {
        const seen = new Set(prev.map(item => item.externalVersionId));
        return [...prev, ...incoming.filter(item => !seen.has(item.externalVersionId))];
      });
      setVersionsHasMore(!!page.hasMore);
      setVersionsTotal(page.total || 0);
      setVersionsVisibleCount(count => count + VERSIONS_PAGE);
    } catch (error) {
      if (request === versionsRequestRef.current) {
        const message = errorMessage(error);
        setVersionsError(message);
      }
    } finally {
      if (request === versionsRequestRef.current) setVersionsLoadingMore(false);
    }
  }

  function openHit(hit: StoreSearchApp) {
    ++versionsRequestRef.current;
    setActionError("");
    const gone = !!hit.removedFromStore;
    const next: CatalogInstallOffer = {
      app_id: `store:${hit.appStoreId}`,
      app_title: hit.name,
      title: hit.name,
      version: "",
      bundle_id: "",
      app_store_id: hit.appStoreId,
      ...(hit.iconUrl ? { icon_url: hit.iconUrl } : {}),
      in_store: !gone && !hit.foundOnAccount,
      source: gone || hit.foundOnAccount ? "restore" : "app_store",
      message: hit.foundOnAccount
        ? "Приложение найдено в покупках текущего Apple ID. Можно скачать IPA или установить на iPhone."
        : gone
        ? "Этого приложения уже нет в App Store. Установить можно, только если оно есть в покупках этого Apple ID."
        : "Версия доступна в App Store и будет привязана к Apple ID перед установкой.",
      show_account_versions: false,
    };
    setOffer(next);
    void loadVersions(next);
  }

  // Signum picks the storefront itself; the only thing left for the user is to
  // agree that the Apple ID country may move for the length of the job.
  function confirmRegionMove(verb: string): boolean {
    if (regionPlan?.action !== "switch") return true;
    const name = regionPlan.targetRegionName || regionPlan.targetRegion || "другую страну";
    return window.confirm(
      `Этого приложения нет в магазине вашей страны.\n\nSignum временно сменит страну Apple ID на «${name}», ${verb}, и сразу вернёт вашу страну обратно.\n\nВажно: если на счету Apple ID есть деньги, сменить страну не получится — проверьте баланс заранее.\n\nПродолжить?`,
    );
  }

  async function queueInstall(selected: CatalogInstallOffer) {
    if (!confirmRegionMove("установит приложение")) return;
    setBusy(true);
    setActionError("");
    try {
      await command<string>("enqueue_catalog_install", {
        input: {
          title: selected.title,
          bundle_id: selected.bundle_id,
          app_store_id: selected.app_store_id,
          in_store: selected.in_store,
          already_owned: selected.source === "restore",
          min_ios: selected.min_ios ?? null,
          version: chosenVersion?.version ?? selected.version,
          external_version_id: chosenVersion?.externalVersionId,
        },
      });
      setOffer(null);
      navigate("/installs");
    } catch (error) {
      setActionError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  /** Закрепить приложение за Apple ID, ничего не скачивая. */
  async function purchaseOnly(selected: CatalogInstallOffer) {
    setPurchasing(true);
    setPurchaseNote("");
    try {
      const message = await command<string>("purchase_app_only", {
        appStoreId: selected.app_store_id ?? null,
        bundleId: selected.bundle_id ?? null,
        title: selected.title,
      });
      setPurchaseNote(message);
    } catch (e) {
      setPurchaseNote(errorMessage(e));
    } finally {
      setPurchasing(false);
    }
  }

  async function queueExport(selected: CatalogInstallOffer) {
    if (!confirmRegionMove("скачает файл")) return;
    setBusy(true);
    setActionError("");
    try {
      await command<string>("enqueue_catalog_export", {
        input: {
          title: selected.title,
          bundle_id: selected.bundle_id,
          app_store_id: selected.app_store_id,
          in_store: selected.in_store,
          already_owned: selected.source === "restore",
          min_ios: selected.min_ios ?? null,
          version: chosenVersion?.version ?? selected.version,
          external_version_id: chosenVersion?.externalVersionId,
        },
      });
      setOffer(null);
      navigate("/installs");
    } catch (error) {
      setActionError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  const tourDemo = useTourDemo();

  // Поиск разработчиков идёт по тому же вводу, что и поиск приложений.
  //
  // Раньше он требовал переключить режим вручную, и человеку приходилось
  // угадывать заранее, студия перед ним или название. Теперь спрашиваем Apple
  // об обоих сразу: приложения показываем выше, студии — ниже, и если студий
  // не нашлось, раздел просто не появляется.
  useEffect(() => {
    const text = query.trim();
    if (text.length < 2) {
      devRequestRef.current += 1;
      setDevHits([]);
      setDevError("");
      setDevSearching(false);
      return;
    }
    const request = ++devRequestRef.current;
    setDevSearching(true);
    const timer = window.setTimeout(() => {
      void searchStoreDevelopers(text)
        .then(hits => {
          if (request !== devRequestRef.current) return;
          setDevHits(hits);
          setDevError("");
        })
        .catch(error => {
          if (request !== devRequestRef.current) return;
          setDevHits([]);
          setDevError(errorMessage(error));
        })
        .finally(() => {
          if (request === devRequestRef.current) setDevSearching(false);
        });
    }, 320);
    return () => window.clearTimeout(timer);
  }, [query]);

  function togglePicked(appStoreId: string) {
    setPicked(prev => {
      const next = new Set(prev);
      if (next.has(appStoreId)) next.delete(appStoreId);
      else next.add(appStoreId);
      return next;
    });
  }

  /// Поставить в очередь все отмеченные приложения разом.
  ///
  /// Ставим по одному и не прерываемся на ошибке: если Apple откажет по
  /// одному приложению, остальные всё равно должны уехать в очередь —
  /// иначе человек не поймёт, что скачалось, а что нет.
  async function queuePicked() {
    if (!picked.size || batchBusy) return;
    setBatchBusy(true);
    setActionError("");
    const chosen = devApps.filter(hit => picked.has(hit.appStoreId));
    logUi("нажал «Скачать отмеченные»", `${chosen.length} шт · ${openDev?.name ?? ""}`);
    const failed: string[] = [];
    for (const hit of chosen) {
      try {
        await command<string>("enqueue_catalog_export", {
          input: {
            title: hit.name,
            bundle_id: "",
            app_store_id: hit.appStoreId,
            in_store: !hit.removedFromStore,
            already_owned: !!hit.removedFromStore,
            min_ios: null,
            version: "",
          },
        });
      } catch (error) {
        failed.push(hit.name);
      }
    }
    setBatchBusy(false);
    if (failed.length === chosen.length) {
      setActionError("Не удалось поставить в очередь ни одно приложение.");
      return;
    }
    if (failed.length) {
      setActionError(`Не удалось поставить в очередь: ${failed.join(", ")}. Остальные скачиваются.`);
    }
    setPicked(new Set());
    navigate("/installs");
  }

  async function openBulkPurchase() {
    setPurchaseError("");
    setPurchaseResult("");
    try {
      const auth = await command<AuthStatus>("get_auth_status");
      const ready = auth.accounts.filter(account => account.session_saved || account.active);
      setPurchaseAccounts(ready);
      setPurchaseAccountEmails(ready.map(account => account.email));
      if (!ready.length) setPurchaseError("Нет сохранённых Apple ID. Войдите в аккаунты на странице «Аккаунт».");
      setPurchaseDialog(true);
    } catch (error) {
      setPurchaseError(errorMessage(error));
      setPurchaseDialog(true);
    }
  }

  async function purchasePickedOnAccounts() {
    const chosen = devApps.filter(hit => picked.has(hit.appStoreId));
    if (!chosen.length || !purchaseAccountEmails.length || purchaseBusy) return;
    if (!window.confirm(
      `Будет выполнена покупка ${chosen.length} приложений на ${purchaseAccountEmails.length} Apple ID. Платные приложения могут привести к списанию средств на каждом аккаунте. Продолжить?`,
    )) return;
    setPurchaseBusy(true);
    setPurchaseError("");
    try {
      const result = await command<{
        results: { email: string; ok: number; failed: string[]; warnings: string[] }[];
        restoredAccount?: string | null;
        restoreError?: string | null;
      }>("purchase_apps_on_accounts", {
        input: {
          confirmed: true,
          items: chosen.map(hit => ({ appStoreId: hit.appStoreId, title: hit.name })),
          accountEmails: purchaseAccountEmails,
        },
      });
      const summary = result.results.map(row => `${row.email}: ${row.ok} куплено${row.failed.length ? `, ошибок ${row.failed.length}` : ""}${row.warnings.length ? `, предупреждений ${row.warnings.length}` : ""}`).join("\n");
      setPurchaseResult(`${summary}${result.restoredAccount ? `\nАктивный Apple ID восстановлен: ${result.restoredAccount}.` : ""}`);
      if (result.restoreError) setPurchaseError(`Не удалось вернуться к исходному Apple ID. Войдите в него на странице «Аккаунт»: ${result.restoreError}`);
      if (result.results.every(row => row.failed.length === 0)) setPicked(new Set());
    } catch (error) {
      setPurchaseError(errorMessage(error));
    } finally {
      setPurchaseBusy(false);
    }
  }

  async function showDeveloper(dev: StoreDeveloper) {
    // Номер запроса: ответ по студии, которую уже закрыли или сменили на
    // другую, не должен подменить её список приложений.
    const request = ++devAppsRequestRef.current;
    setOpenDev(dev);
    setPicked(new Set());
    setDevApps([]);
    setDevAppsError("");
    setDevAppsLoading(true);
    try {
      const apps = await developerApps(dev.artistId);
      if (request === devAppsRequestRef.current) setDevApps(apps);
    } catch (e) {
      if (request === devAppsRequestRef.current) setDevAppsError(errorMessage(e));
    } finally {
      if (request === devAppsRequestRef.current) setDevAppsLoading(false);
    }
  }

  useEffect(() => {
    onSearchingChange?.(storeSearching || devSearching);
  }, [storeSearching, devSearching, onSearchingChange]);
  useEffect(() => () => onSearchingChange?.(false), [onSearchingChange]);

  const shownHits = tourShowHits ? tourSearchHits : storeHits;

  // Одно приложение — одна карточка. Если App Store ID результата совпадает с
  // приложением каталога, показываем карточку каталога (у неё проверенные
  // версии и восстановление из покупок), а если она уже есть выше — не
  // дублируем её вовсе.
  const catalogByStoreId = useMemo(() => {
    const map = new Map<string, CatalogApp>();
    for (const app of catalogApps) {
      for (const id of app.app_store_ids ?? []) {
        const key = normalizeStoreId(id);
        if (key && !map.has(key)) map.set(key, app);
      }
    }
    return map;
  }, [catalogApps]);
  // Запасной путь для карточек без App Store ID (старый сервер, приложение
  // снято с продажи): сверяем bundle id, если iTunes его отдал.
  const catalogByBundleId = useMemo(() => {
    const map = new Map<string, CatalogApp>();
    for (const app of catalogApps) {
      if ((app.app_store_ids ?? []).some(id => normalizeStoreId(id))) continue;
      for (const bundle of app.bundle_ids ?? []) {
        const key = normalizeBundleId(bundle);
        if (key && !map.has(key)) map.set(key, app);
      }
    }
    return map;
  }, [catalogApps]);
  const mergedHits = useMemo(() => {
    const seenCatalog = new Set<string>();
    const seenStore = new Set<string>();
    const out: Array<{ hit: StoreSearchApp; catalog: CatalogApp | null }> = [];
    for (const hit of shownHits) {
      const storeId = normalizeStoreId(hit.appStoreId);
      // AppMagic и iTunes могут вернуть одно и то же приложение дважды:
      // показываем его один раз, в порядке первого появления.
      if (storeId) {
        if (seenStore.has(storeId)) continue;
        seenStore.add(storeId);
      }
      const bundle = normalizeBundleId(hit.bundleId);
      const catalog = tourShowHits
        ? null
        : (storeId ? catalogByStoreId.get(storeId) : undefined)
          ?? (bundle ? catalogByBundleId.get(bundle) : undefined)
          ?? null;
      if (catalog) {
        if (shownCatalogIds.has(catalog.id) || seenCatalog.has(catalog.id)) continue;
        seenCatalog.add(catalog.id);
      }
      out.push({ hit, catalog });
    }
    return out;
  }, [catalogByBundleId, catalogByStoreId, shownCatalogIds, shownHits, tourShowHits]);
  const allMergedAbove = !storeSearching && shownHits.length > 0 && mergedHits.length === 0;
  const typed = query.trim().length >= 2;
  const showEmpty = typed && !storeSearching && shownHits.length === 0 && !storeError && !tourDemo;
  // Первый поиск: результатов ещё нет — рисуем заглушки карточек, чтобы
  // раздел не «прыгал», когда ответ придёт.
  const showSkeleton = typed && storeSearching && shownHits.length === 0;
  // Повторный поиск: старые результаты остаются на месте, но приглушены,
  // чтобы было видно, что они относятся к прошлому запросу.
  const staleHits = storeSearching && shownHits.length > 0;

  return (
    <div>
      {storeError && (
        <p className="mt-4 rounded-xl border border-rose-400/15 bg-rose-400/5 px-4 py-3 text-xs text-rose-300">
          {storeError}
        </p>
      )}

      {(devError || devAppsError) && (
        <p className="mt-4 rounded-xl border border-rose-400/15 bg-rose-400/5 px-4 py-3 text-xs text-rose-300">
          {devError || devAppsError}
        </p>
      )}
      {/* ── Страница разработчика ─────────────────────────────────────── */}
      {openDev ? (
        <div className="mt-6">
          {/* Шапка студии: иконки её приложений, имя, счёт. Раньше здесь была
              кнопка «Назад» вплотную к тексту — читалось как случайный набор. */}
          <div className="panel flex items-center gap-4 p-4">
            <button
              type="button"
              onClick={() => { ++devAppsRequestRef.current; setOpenDev(null); setDevAppsLoading(false); }}
              className="shrink-0 rounded-lg p-2 text-slate-400 transition hover:bg-white/[0.06] hover:text-white"
              aria-label="Назад к поиску"
            >
              <ArrowLeft size={18} />
            </button>
            <div className="flex shrink-0 -space-x-2">
              {(openDev.icons.length ? openDev.icons : [""]).slice(0, 3).map((icon, k) => (
                <div key={k} className="rounded-xl ring-2 ring-[color:var(--app-surface)]">
                  <ResultIcon url={icon} name={openDev.name} small />
                </div>
              ))}
            </div>
            <div className="min-w-0 flex-1">
              <div className="truncate text-base font-bold text-slate-100">{openDev.name}</div>
              <div className="mt-0.5 text-xs text-slate-500">
                {devAppsLoading
                  ? "Спрашиваем Apple…"
                  : `${devApps.length} ${pluralApps(devApps.length)} в App Store`}
              </div>
            </div>
          </div>

          {/* Панель выбора появляется только когда есть что выбирать — пустая
              строка с нулём отмеченных была бы шумом. */}
          {!devAppsLoading && devApps.length > 0 && (
            <div className="mt-3 flex flex-wrap items-center gap-3">
              <button
                type="button"
                onClick={() =>
                  setPicked(prev =>
                    prev.size === devApps.length
                      ? new Set()
                      : new Set(devApps.map(hit => hit.appStoreId)))
                }
                className="btn-secondary px-3 py-2 text-xs"
              >
                {picked.size === devApps.length ? "Снять все" : "Выбрать все"}
              </button>

              <button
                type="button"
                disabled={!picked.size || batchBusy}
                onClick={() => void queuePicked()}
                className="btn-primary px-4 py-2 text-xs disabled:opacity-40"
              >
                {batchBusy ? (
                  <>
                    <LoaderCircle size={14} className="animate-spin" />
                    Ставим в очередь…
                  </>
                ) : (
                  <>
                    <Download size={14} />
                    Скачать отмеченные{picked.size ? ` · ${picked.size}` : ""}
                  </>
                )}
              </button>

              <button
                type="button"
                disabled={!picked.size || batchBusy || purchaseBusy}
                onClick={() => void openBulkPurchase()}
                className="btn-secondary px-4 py-2 text-xs disabled:opacity-40"
              >
                <ShoppingBag size={14} />Купить на Apple ID{picked.size ? ` · ${picked.size}` : ""}
              </button>

              <span className="text-[11px] leading-4 text-slate-500">
                Apple отдаёт только то, что сейчас в магазине. Снятые с продажи
                ищите по названию или номеру.
              </span>
            </div>
          )}

          <div className="mt-4 space-y-2">
            {devAppsLoading && (
              <div className="panel flex items-center gap-3 px-4 py-6 text-sm text-slate-400">
                <LoaderCircle className="animate-spin text-brand-300" size={16} />
                Спрашиваем Apple…
              </div>
            )}

            {devApps.map(hit => {
              const on = picked.has(hit.appStoreId);
              return (
                <div
                  key={hit.appStoreId}
                  className={`panel flex items-center gap-3 p-3 transition ${
                    on ? "border-brand-400/35 bg-brand-500/[0.07]" : "hover:border-white/[0.12] hover:bg-white/[0.04]"
                  }`}
                >
                  {/* Отметка отдельной кнопкой: нажатие на строку должно
                      открывать приложение, а не молча его отмечать. */}
                  <button
                    type="button"
                    onClick={() => togglePicked(hit.appStoreId)}
                    className="shrink-0"
                    aria-label={on ? `Снять отметку с ${hit.name}` : `Отметить ${hit.name}`}
                    aria-pressed={on}
                  >
                    <span
                      className={`grid h-5 w-5 place-items-center rounded-md border transition ${
                        on
                          ? "border-brand-400 bg-brand-500 text-white"
                          : "border-white/20 text-transparent hover:border-white/40"
                      }`}
                    >
                      <Check size={13} strokeWidth={3} />
                    </span>
                  </button>

                  <button
                    type="button"
                    className="flex min-w-0 flex-1 items-center gap-3 text-left"
                    onClick={() => openHit(hit)}
                  >
                    <ResultIcon url={hit.iconUrl} name={hit.name} />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-sm font-semibold text-slate-100">{hit.name}</div>
                      <div className="mt-0.5 truncate text-xs text-slate-500">{hit.appStoreId}</div>
                    </div>
                    <Download size={15} className="shrink-0 text-slate-500" />
                  </button>
                </div>
              );
            })}
          </div>
        </div>
      ) : (
        <>
          {/* ── Приложения ─────────────────────────────────────────────── */}
          <div
            aria-busy={storeSearching}
            className={`mt-2 grid grid-cols-[repeat(auto-fill,minmax(230px,1fr))] gap-4 transition-opacity duration-200 ${staleHits ? "opacity-60" : ""}`}
          >
            {showSkeleton && Array.from({ length: 4 }, (_, k) => <CardSkeleton key={`skeleton-${k}`} />)}

            {mergedHits.map(({ hit, catalog }, i) => (
              catalog ? (
                <AppCard
                  key={`catalog-${catalog.id}`}
                  app={catalog}
                  onClick={onOpenCatalogApp}
                  favorite={favoriteIds?.includes(catalog.id)}
                  onToggleFavorite={onToggleFavorite}
                />
              ) : (
                <StoreHitCard
                  key={`store-${normalizeStoreId(hit.appStoreId) || hit.appStoreId}-${i}`}
                  hit={hit}
                  dataTour={tourShowHits && i === 0 ? "find-hit" : undefined}
                  onClick={() => { if (!tourShowHits) openHit(hit); }}
                />
              )
            ))}

            {allMergedAbove && (
              <div className="panel col-span-full px-6 py-6 text-center text-xs text-slate-500">
                Всё, что нашлось в App Store, уже есть в каталоге выше.
              </div>
            )}

            {showEmpty && devHits.length === 0 && (
              <div className="panel col-span-full px-6 py-10 text-center">
                <Search className="mx-auto text-slate-600" size={28} />
                <p className="mt-3 text-sm font-semibold text-slate-300">Ничего не найдено</p>
                <p className="mt-1 text-xs text-slate-600">
                  Попробуйте другое название, ссылку из App Store или номер.
                </p>
              </div>
            )}

            {!typed && !storeSearching && (
              <div className="panel col-span-full px-6 py-10 text-center">
                <Search className="mx-auto text-slate-600" size={28} />
                <p className="mt-3 text-sm font-semibold text-slate-300">Что найти</p>
                <p className="mt-1 text-xs leading-5 text-slate-600">
                  Название, ссылка из App Store, номер приложения или имя разработчика.
                </p>
              </div>
            )}
          </div>

          {/* ── Разработчики ───────────────────────────────────────────────
              Ниже приложений и только если нашлись: чаще ищут приложение, а
              студию — изредка. Пустого раздела быть не должно. */}
          {devHits.length > 0 && (
            <div className="mt-8">
              <div className="flex items-baseline gap-2">
                <h2 className="text-sm font-bold text-slate-200">Разработчики</h2>
                <span className="text-xs text-slate-500">
                  {devHits.length} {pluralDevs(devHits.length)}
                </span>
              </div>
              <div className="mt-3 space-y-2">
                {devHits.map(dev => (
                  <button
                    key={dev.artistId}
                    type="button"
                    onClick={() => void showDeveloper(dev)}
                    className="panel flex w-full items-center gap-3 p-3 text-left transition hover:border-white/[0.12] hover:bg-white/[0.04]"
                  >
                    <div className="flex shrink-0 -space-x-2">
                      {(dev.icons.length ? dev.icons : [""]).slice(0, 3).map((icon, k) => (
                        <div key={k} className="rounded-lg ring-2 ring-[color:var(--app-surface)]">
                          <ResultIcon url={icon} name={dev.name} small />
                        </div>
                      ))}
                    </div>
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-sm font-semibold text-slate-100">{dev.name}</div>
                      <div className="mt-0.5 text-xs text-slate-500">
                        {dev.appCount} {pluralApps(dev.appCount)} в App Store
                      </div>
                    </div>
                    <ChevronRight size={16} className="shrink-0 text-slate-600" />
                  </button>
                ))}
              </div>
            </div>
          )}
        </>
      )}

      {offer && (
        <div className="fixed inset-0 z-50 flex items-center justify-center overflow-y-auto bg-black/70 p-6 backdrop-blur-md" onMouseDown={() => !busy && setOffer(null)}>
          <div className="panel max-h-[calc(100vh-3rem)] w-full max-w-lg overflow-y-auto overscroll-contain p-6" onMouseDown={event => event.stopPropagation()}>
            <div className="flex items-start justify-between gap-4">
              <div className="flex items-center gap-4">
                <ResultIcon url={offer.icon_url || ""} name={offer.title} large />
                <div>
                  <h2 className="text-xl font-bold">{offer.app_title}</h2>
                  <p className="mt-1 text-xs text-slate-500">
                    {formatVersionWithIos(chosenVersion?.version ?? offer.version, offer.min_ios)}
                  </p>
                </div>
              </div>
              <button
                className="rounded-lg p-2 text-slate-500 hover:bg-white/5 hover:text-white"
                onClick={() => setOffer(null)}
                disabled={busy}
              >
                <X size={18} />
              </button>
            </div>

            <div className={`mt-5 flex items-start gap-3 rounded-xl border p-4 ${
              offer.source === "app_store"
                ? "border-emerald-400/20 bg-emerald-400/10"
                : "border-amber-400/20 bg-amber-400/10"
            }`}>
              <Store size={18} className="mt-0.5 text-brand-300" />
              <div>
                <div className="text-sm font-semibold text-slate-100">
                  {offer.source === "app_store" ? "App Store" : "Нет в App Store"}
                </div>
                <p className="mt-1 text-xs leading-5 text-slate-400">{offer.message}</p>
              </div>
            </div>

            <div className="mt-5 rounded-xl border border-white/[0.08] p-3">
              <div className="flex items-center justify-between gap-3">
                <span className="text-xs font-semibold text-slate-200">Версия для скачивания</span>
                <button
                  className="text-[11px] text-brand-300 hover:text-white disabled:opacity-50"
                  disabled={versionsLoading || !offer.app_store_id}
                  onClick={() => void loadVersions(offer, true)}
                >
                  Обновить
                </button>
              </div>
              {versionsLoading ? (
                <div className="mt-3 flex items-center gap-2 text-xs text-slate-500">
                  <LoaderCircle size={14} className="animate-spin text-brand-300" />
                  Загружаем список версий…
                </div>
              ) : versionsError ? (
                <div className="mt-2 space-y-2">
                  <p className="text-xs leading-5 text-rose-300 whitespace-pre-line">{versionsError}</p>
                  <p className="text-[11px] leading-4 text-slate-500">
                    {versionsNeedsAuth
                      ? "Apple запросила вход или код подтверждения. Поле кода находится на странице аккаунта."
                      : "Проверьте вход в Apple ID и повторите запрос."}
                  </p>
                  <button
                    type="button"
                    className="text-[11px] font-medium text-brand-300 hover:text-white"
                    onClick={() => navigate("/account")}
                  >
                    {versionsNeedsAuth ? "Ввести код в «Аккаунте»" : "Открыть «Аккаунт»"}
                  </button>
                </div>
              ) : appStoreVersions.length === 0 ? (
                // An empty list has two very different causes, and "пусто"
                // alone reads like a failure in both.
                versionsRegionBlocked ? (
                  <p className="mt-2 text-xs leading-5 text-amber-200/80">
                    Приложение есть в покупках, но его нет в магазине вашей страны — Apple не отдаёт
                    список версий. Скачать можно только последнюю.
                  </p>
                ) : (
                  <p className="mt-2 text-xs text-slate-500">
                    Приложения ещё нет в покупках этого Apple ID — история версий появится после
                    установки. Сейчас будет скачана последняя версия.
                  </p>
                )
              ) : (
                <>
                  <label className="relative mt-2 block">
                    <Search size={13} className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-slate-500" />
                    <input
                      type="search"
                      className="field w-full py-2 pl-8 pr-8 text-xs"
                      placeholder="Поиск"
                      value={versionsSearch}
                      onChange={event => {
                        setVersionsSearch(event.target.value);
                        setVersionsVisibleCount(VERSIONS_PAGE);
                      }}
                    />
                  </label>
                  <div className="mt-2 max-h-52 space-y-1 overflow-y-auto overscroll-contain pr-0.5">
                    {filteredVersions.slice(0, versionsVisibleCount).map(version => (
                      <label
                        key={version.externalVersionId}
                        className="flex cursor-pointer items-center justify-between gap-3 rounded-lg px-2 py-1.5 text-xs hover:bg-white/[0.05]"
                      >
                        <span className="flex min-w-0 items-center gap-2">
                          <input
                            type="radio"
                            name="find-store-version"
                            checked={chosenVersion?.externalVersionId === version.externalVersionId}
                            onChange={() => setChosenVersion(version)}
                          />
                          <span className="min-w-0 truncate">
                            {formatAppStoreVersionLabel(version.version, version.externalVersionId)}
                            {/* Apple выпускает одну маркетинговую версию несколькими
                                сборками — без номера такие строки неразличимы. */}
                            {!formatAppStoreVersionLabel(version.version, version.externalVersionId).includes(version.externalVersionId) && (
                              <span className="ml-1.5 text-[10px] text-slate-400">{version.externalVersionId}</span>
                            )}
                          </span>
                          {version.isLatest && <span className="shrink-0 text-brand-300">Последняя</span>}
                        </span>
                        <span className="flex shrink-0 items-center gap-2">
                          {version.minIos && <span className="text-slate-500">{formatMinIos(version.minIos)}</span>}
                          {!version.minIos && iosLoadingIds.has(version.externalVersionId) && (
                            <span className="inline-flex items-center gap-1 text-slate-500" role="status">
                              <LoaderCircle size={12} className="animate-spin" />iOS…
                            </span>
                          )}
                        </span>
                      </label>
                    ))}
                  </div>
                  {versionsVisibleCount < filteredVersions.length && (
                    <button
                      type="button"
                      className="mt-2 w-full rounded-lg border border-white/[0.08] py-2 text-[11px] font-semibold text-brand-300"
                      onClick={() => setVersionsVisibleCount(count => count + VERSIONS_PAGE)}
                    >
                      Показать ещё в списке
                    </button>
                  )}
                  {versionsHasMore && !versionsSearch.trim() && (
                    <button
                      type="button"
                      className="mt-2 w-full rounded-lg border border-white/[0.08] py-2 text-[11px] font-semibold text-brand-300 disabled:opacity-50"
                      disabled={versionsLoadingMore}
                      onClick={() => void loadMoreVersions(offer)}
                    >
                      {versionsLoadingMore ? (
                        <span className="inline-flex items-center justify-center gap-2">
                          <LoaderCircle size={13} className="animate-spin" />
                          Загружаем ещё…
                        </span>
                      ) : (
                        `Загрузить ещё${versionsTotal > appStoreVersions.length ? ` · осталось ${versionsTotal - appStoreVersions.length}` : ""}`
                      )}
                    </button>
                  )}
                </>
              )}
            </div>

            {actionError && (
              <p className="mt-4 text-xs leading-5 text-rose-300">{actionError}</p>
            )}

            <div className="mt-5 space-y-2">
              {regionPlanBusy && !regionPlan && (
                <p className="flex items-center gap-2 px-1 text-[11px] text-slate-500">
                  <LoaderCircle size={11} className="animate-spin text-brand-300" />
                  Проверяем, где это приложение продаётся…
                </p>
              )}
              {regionPlan?.action === "switch" && (
                <div className="rounded-xl border border-amber-400/25 bg-amber-400/10 px-3 py-2.5">
                  <div className="text-[11px] font-semibold text-amber-200">Понадобится временная смена страны</div>
                  <p className="mt-1 text-[11px] leading-4 text-amber-100/80">{regionPlan.message}</p>
                  <p className="mt-1 text-[11px] leading-4 text-amber-100/60">
                    Если на счету Apple ID есть деньги, сменить страну не получится — проверьте баланс заранее.
                  </p>
                </div>
              )}
              {regionPlan?.action === "unavailable" && (
                <p className="rounded-xl border border-rose-400/25 bg-rose-400/10 px-3 py-2.5 text-[11px] leading-4 text-rose-200">
                  {regionPlan.message}
                </p>
              )}
              {(regionPlan?.action === "owned" || regionPlan?.action === "purchase") && (
                <p className="px-1 text-[11px] leading-4 text-slate-500">{regionPlan.message}</p>
              )}
              <button className="btn-primary w-full" disabled={busy} onClick={() => void queueInstall(offer)}>
                {busy ? <LoaderCircle size={15} className="animate-spin" /> : <Smartphone size={15} />}
                Установить на iPhone
              </button>
              <button className="btn-secondary w-full" disabled={busy} onClick={() => void queueExport(offer)}>
                <Download size={15} />
                Скачать файл
              </button>
              <button className="btn-secondary w-full" disabled={busy} onClick={() => setOffer(null)}>
                Отмена
              </button>
            </div>
          </div>
        </div>
      )}

      {purchaseDialog && (
        <div className="fixed inset-0 z-[70] grid place-items-center bg-black/70 p-6 backdrop-blur-md" onMouseDown={() => !purchaseBusy && setPurchaseDialog(false)}>
          <section className="panel max-h-[85vh] w-full max-w-lg overflow-y-auto p-6" onMouseDown={event => event.stopPropagation()}>
            <header className="flex items-start justify-between gap-4">
              <div>
                <h2 className="text-lg font-bold">Покупка на нескольких Apple ID</h2>
                <p className="mt-1 text-xs leading-5 text-slate-500">
                  Выбрано приложений: {picked.size}. Выберите сохранённые Apple ID для покупки.
                </p>
              </div>
              <button className="rounded-lg p-2 text-slate-500 hover:bg-white/5 hover:text-white" disabled={purchaseBusy} onClick={() => setPurchaseDialog(false)} aria-label="Закрыть"><X size={18} /></button>
            </header>
            <div className="mt-4 rounded-xl border border-amber-400/20 bg-amber-400/[0.06] p-3 text-xs leading-5 text-amber-100/80">
              Платные приложения могут списать деньги отдельно с каждого Apple ID. Проверьте регион и способ оплаты аккаунтов перед покупкой.
            </div>
            {purchaseAccounts.length > 0 && (
              <div className="mt-4 space-y-2">
                <div className="flex items-center gap-2">
                  <Checkbox
                    checked={purchaseAccountEmails.length === purchaseAccounts.length}
                    onChange={checked => setPurchaseAccountEmails(checked ? purchaseAccounts.map(account => account.email) : [])}
                    disabled={purchaseBusy}
                    label="Выбрать все сохранённые аккаунты"
                  />
                </div>
                {purchaseAccounts.map(account => (
                  <div key={account.email} className="flex items-center gap-3 rounded-xl border border-white/[0.06] bg-black/10 px-3 py-2.5">
                    <Checkbox
                      checked={purchaseAccountEmails.includes(account.email)}
                      onChange={checked => setPurchaseAccountEmails(current => checked
                        ? [...current, account.email]
                        : current.filter(email => email !== account.email))}
                      disabled={purchaseBusy}
                      aria-label={`Выбрать ${account.email}`}
                    />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-xs font-semibold text-slate-200">{account.email}</div>
                      <div className="text-[10px] text-slate-600">{account.active ? "Активный аккаунт" : "Сохранённый аккаунт"}</div>
                    </div>
                  </div>
                ))}
              </div>
            )}
            {purchaseError && <p role="alert" className="mt-4 whitespace-pre-wrap text-xs leading-5 text-rose-300">{purchaseError}</p>}
            {purchaseResult && <p role="status" className="mt-4 whitespace-pre-wrap rounded-xl border border-emerald-400/15 bg-emerald-400/[0.05] p-3 text-xs leading-5 text-emerald-200">{purchaseResult}</p>}
            <div className="mt-5 flex flex-wrap justify-end gap-2">
              <button className="btn-secondary" disabled={purchaseBusy} onClick={() => setPurchaseDialog(false)}>Закрыть</button>
              <button className="btn-primary" disabled={purchaseBusy || !purchaseAccountEmails.length || !picked.size} onClick={() => void purchasePickedOnAccounts()}>
                {purchaseBusy ? <LoaderCircle size={14} className="animate-spin" /> : <ShoppingBag size={14} />}
                {purchaseBusy ? "Покупаем…" : "Подтвердить покупку"}
              </button>
            </div>
          </section>
        </div>
      )}
    </div>
  );
}
