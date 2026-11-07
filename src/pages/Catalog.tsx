import { AlertTriangle, Check, ChevronDown, Download, FolderOpen, GraduationCap, History, ListChecks, LoaderCircle, QrCode, RefreshCw, RotateCcw, Search, Smartphone, Star, Store, Wand2, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import ActivationModal from "../components/ActivationModal";
import AppCard from "../components/AppCard";
import BankAltInstallModal from "../components/BankAltInstallModal";
import LoadingSteps from "../components/LoadingSteps";
import PageHeader from "../components/PageHeader";
import StatusMessage from "../components/StatusMessage";
import TestFlightModal from "../components/TestFlightModal";
import {
  activationForInstalledVersion,
  resolveAppActivation,
  storePendingPostInstallActivation,
} from "../lib/activation";
import { bankDisplayName, bankIconUrl, detectBankFromApp, detectBankKey } from "../lib/bankAlt";
import { searchCatalogApps } from "../lib/catalogSearch";
import { formatMinIos, formatVersionWithIos } from "../lib/format";
import { command, errorMessage, logUi, fetchAppStoreVersions, resolveAppStoreVersionsIos, namedAppStoreVersions } from "../lib/tauri";
import {
  hideTourOffer,
  resetOnboarding,
  TOUR_APP_ID,
  TOUR_DEVICE_IOS,
  tourCatalogApp,
  tourInstallOffer,
  tourOfferHidden,
  tourVersions,
  useTourDemo,
  useTourSignal,
} from "../lib/tourStore";
import type {
  AccountRestoreScan,
  AccountRestoreVersion,
  AppActivation,
  AppStoreVersion,
  BankKey,
  CatalogApp,
  CatalogBanner,
  CatalogFeed,
  CatalogGroup,
  CatalogInstallOffer,
} from "../types";
import type { AppSettings } from "../types";

const HAPP_APP_STORE_ID = "6504287215";

function restoreItemToOffer(item: AccountRestoreVersion, deviceIos?: string | null): CatalogInstallOffer {
  const description = item.description?.trim();
  const incompatible = item.device_compatible === false;
  const baseMessage = item.in_store
    ? "Версия доступна в App Store и будет привязана к Apple ID перед установкой."
    : "Этой версии уже нет в App Store. Установить можно только если она уже есть в покупках Apple ID.";
  const iosNote = incompatible
    ? ` Телефон не поддерживает эту версию (нужна iOS ${item.min_ios ?? "новее"}${deviceIos ? `, у вас ${deviceIos}` : ""}). Установить можно, но приложение не запустится.`
    : "";
  return {
    app_id: item.app_id,
    app_title: item.app_title,
    title: item.title,
    ...(description ? { description } : {}),
    version: item.version,
    bundle_id: item.bundle_id,
    app_store_id: item.app_store_id,
    min_ios: item.min_ios,
    icon_url: item.icon_url,
    in_store: item.in_store,
    source: item.in_store ? "app_store" : "restore",
    message: `${baseMessage}${iosNote}`,
    show_account_versions: false,
    installed_on_device: item.installed_on_device,
    installed_version: item.installed_version,
    device_compatible: item.device_compatible,
    device_ios: deviceIos ?? undefined,
  };
}

const OPEN_STEPS = [
  "Открываем…",
  "Спрашиваем Apple…",
  "Проверяем версии…",
  "Ждём Apple — иногда это пара минут…",
];

const RESTORE_SCAN_STEPS = [
  "Ищем версии в покупках…",
  "Спрашиваем Apple…",
  "Собираем список…",
  "Ждём Apple — иногда это пара минут…",
];

const RESTORE_REFRESH_STEPS = [
  "Обновляем список…",
  "Спрашиваем Apple…",
  "Собираем список…",
  "Ждём Apple — иногда это пара минут…",
];

export default function Catalog() {
  const [apps, setApps] = useState<CatalogApp[]>([]);
  const [groups, setGroups] = useState<CatalogGroup[]>([]);
  const [banners, setBanners] = useState<CatalogBanner[]>([]);
  const [offlineSince, setOfflineSince] = useState<number | null>(null);
  const [groupFilter, setGroupFilter] = useState<string>("all");
  const [catalogSort, setCatalogSort] = useState<"default" | "name" | "ios">("default");
  const [compatibilityFilter, setCompatibilityFilter] = useState<"all" | "compatible" | "incompatible">("all");
  const [searchQuery, setSearchQuery] = useState("");
  const [favorites, setFavorites] = useState<string[]>([]);
  const [selectMode, setSelectMode] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [bulkBusy, setBulkBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [offer, setOffer] = useState<CatalogInstallOffer | null>(null);
  const [testFlightApp, setTestFlightApp] = useState<CatalogApp | null>(null);
  const [resolveError, setResolveError] = useState("");
  const [detailLoading, setDetailLoading] = useState(false);
  const [loadingSteps, setLoadingSteps] = useState<string[]>(OPEN_STEPS);
  const [deviceIos, setDeviceIos] = useState<string | null>(null);
  const [appRestoreScan, setAppRestoreScan] = useState<AccountRestoreScan | null>(null);
  const [appRestoreOpen, setAppRestoreOpen] = useState(false);
  const [appStoreVersions, setAppStoreVersions] = useState<AppStoreVersion[]>([]);
  const [versionsLoading, setVersionsLoading] = useState(false);
  const [versionsLoadingMore, setVersionsLoadingMore] = useState(false);
  const [iosLoadingIds, setIosLoadingIds] = useState<Set<string>>(new Set());
  const [versionsError, setVersionsError] = useState("");
  const [versionsHasMore, setVersionsHasMore] = useState(false);
  const [versionsTotal, setVersionsTotal] = useState(0);
  const [chosenVersion, setChosenVersion] = useState<AppStoreVersion | null>(null);
  const [fitBusy, setFitBusy] = useState(false);

  const [versionsVisibleCount, setVersionsVisibleCount] = useState(12);
  const [restoreVisibleCount, setRestoreVisibleCount] = useState(12);
  const [versionsSearch, setVersionsSearch] = useState("");
  const [restoreVersionsSearch, setRestoreVersionsSearch] = useState("");
  const VERSIONS_PAGE = 12;
                                                                      
  const offerRequestRef = useRef(0);
  const versionsRequestRef = useRef(0);
  const versionsOffsetRef = useRef(0);
  const [activation, setActivation] = useState<{ appTitle: string; data: AppActivation } | null>(null);
  const [exportQueued, setExportQueued] = useState(() => sessionStorage.getItem("signum-export-queued") === "1");
  const [bankAlt, setBankAlt] = useState<BankKey | null>(null);
  const navigate = useNavigate();
  const tourDemo = useTourDemo();
  const tourSignal = useTourSignal();
  const [tourOffer, setTourOffer] = useState(() => !tourOfferHidden());
  // Обучение могло закончиться, пока страница смонтирована — перечитываем флаг.
  useEffect(() => {
    if (!tourDemo) setTourOffer(!tourOfferHidden());
  }, [tourDemo]);
  // В обучении телефон не подключён, поэтому совместимость показываем на демо-iOS.
  const deviceIosUi = tourDemo ? TOUR_DEVICE_IOS : deviceIos;

  // The walkthrough drives this page's real sheet instead of drawing a replica,
  // using local demo data so nothing is asked of Apple.
  useEffect(() => {
    if (!tourDemo) return;
    if (tourSignal.name === "offer-loading") {
      setOffer(null);
      setLoadingSteps(OPEN_STEPS);
      setDetailLoading(true);
      return;
    }
    if (tourSignal.name === "offer-open") {
      setDetailLoading(false);
      setResolveError("");
      setAppStoreVersions(tourVersions);
      setChosenVersion(tourVersions[0]);
      setVersionsLoading(false);
      setVersionsError("");
      setVersionsHasMore(false);
      setVersionsTotal(tourVersions.length);
      setOffer(tourInstallOffer);
      return;
    }
    if (tourSignal.name === "offer-close") {
      setDetailLoading(false);
      setOffer(null);
    }
  }, [tourDemo, tourSignal]);

  // Leaving the tour must not strand demo state on the page.
  useEffect(() => {
    if (tourDemo) return;
    setOffer(current => (current?.app_id === TOUR_APP_ID ? null : current));
    setAppStoreVersions(current => (current === tourVersions ? [] : current));
  }, [tourDemo]);

  const loadDeviceIos = useCallback(async () => {
    try {
      const device = await command<{ product_version?: string }>("get_device_info");
      setDeviceIos(device.product_version ?? null);
    } catch {
      setDeviceIos(null);
    }
  }, []);

  const load = useCallback(async () => {
    setLoading(true); setError("");
    try {
                                                                        
      setAppRestoreScan(null);
      setOffer(null);
      setAppRestoreOpen(false);
      const feed = await command<CatalogFeed>("fetch_catalog_feed");
      setApps(feed.apps ?? []);
      setGroups(feed.groups ?? []);
      setBanners(feed.banners ?? []);
      setOfflineSince(feed.from_cache ? (feed.cached_at ?? 0) : null);
      // Not awaited: probing the iPhone runs ideviceinfo up to three times
      // (saved UDID, USB, Wi-Fi) and held the catalogue spinner for seconds.
      // Only the subtitle and the iOS-compatibility hints read it, and both
      // render fine until it lands.
      void loadDeviceIos();
    }
    catch (e) { setError(errorMessage(e)); }
    finally { setLoading(false); }
  }, [loadDeviceIos]);
  useEffect(() => { void load(); }, [load]);

  const offerActivation = useMemo(() => {
    if (!offer) return null;
    const fromCard = apps.find(a => a.id === offer.app_id)?.activation;
    return resolveAppActivation(fromCard);
  }, [offer, apps]);

  const filteredAppStoreVersions = useMemo(
    () => appStoreVersions.filter(v => versionMatchesSearch(v.version, v.externalVersionId, versionsSearch)),
    [appStoreVersions, versionsSearch],
  );

  const filteredRestoreVersions = useMemo(() => {
    const items = appRestoreScan?.items ?? [];
    const q = restoreVersionsSearch.trim().toLowerCase();
    if (!q) return items;
    return items.filter(item => {
      const hay = [
        item.version,
        item.app_store_id,
        item.bundle_id,
        item.title,
        item.description,
      ]
        .filter(Boolean)
        .join(" ")
        .toLowerCase();
      return hay.includes(q) || (item.app_store_id || "").includes(q.replace(/\s/g, ""));
    });
  }, [appRestoreScan, restoreVersionsSearch]);


  async function pickVersionForDevice(selected: CatalogInstallOffer) {
    const adamId = selected.app_store_id?.trim();
    if (!adamId || !deviceIos || fitBusy) return;
    setFitBusy(true);
    setVersionsError("");
    try {
      const found = await command<AppStoreVersion | null>("find_app_store_version_for_device", {
        input: { adamId, deviceIos },
      });
      if (!found) {
        setVersionsError(`Ни одна версия из покупок не запустится на iOS ${deviceIos}.`);
        return;
      }
      setAppStoreVersions(prev => prev.some(item => item.externalVersionId === found.externalVersionId)
        ? prev.map(item => item.externalVersionId === found.externalVersionId ? { ...item, ...found } : item)
        : [found, ...prev]);
      setChosenVersion(found);
      setVersionsSearch("");
    } catch (e) {
      setVersionsError(errorMessage(e));
    } finally {
      setFitBusy(false);
    }
  }

  useEffect(() => {
    void command<AppSettings>("get_settings")
      .then(settings => setFavorites(settings.favorite_app_ids ?? []))
      .catch(() => undefined);
  }, []);

  async function toggleFavorite(appId: string) {
    try {
      setFavorites(await command<string[]>("toggle_favorite_app", { appId }));
    } catch (e) {
      setResolveError(errorMessage(e));
    }
  }

  // Read through a ref so the handlers below can stay referentially stable.
  const latestRef = useRef({ selectMode, openInstallOffer, toggleFavorite });
  latestRef.current = { selectMode, openInstallOffer, toggleFavorite };

  const onCardClick = useCallback((app: CatalogApp) => {
    if (!latestRef.current.selectMode) {
      void latestRef.current.openInstallOffer(app);
      return;
    }
    setSelectedIds(prev => {
      const next = new Set(prev);
      if (!next.delete(app.id)) next.add(app.id);
      return next;
    });
  }, []);

  const onCardFavorite = useCallback((app: CatalogApp) => {
    void latestRef.current.toggleFavorite(app.id);
  }, []);

  async function installSelected() {
    if (selectedIds.size === 0 || bulkBusy) return;
    setBulkBusy(true);
    try {
      const result = await command<{ jobIds: string[]; skipped: { appId: string; reason: string }[] }>(
        "enqueue_catalog_installs",
        { input: { appIds: Array.from(selectedIds) } },
      );
      if (result.jobIds.length > 0) {
        setSelectMode(false);
        setSelectedIds(new Set());
        navigate("/installs");
        return;
      }
      setResolveError(result.skipped[0]?.reason || "Ничего не удалось поставить в очередь.");
    } catch (e) {
      setResolveError(errorMessage(e));
    } finally {
      setBulkBusy(false);
    }
  }

  async function resolveOffer(appId: string, restoreOnly: boolean) {
    return command<CatalogInstallOffer>("resolve_catalog_install", {
      input: { appId, restoreOnly },
    });
  }

  async function openInstallOffer(app: CatalogApp) {
    logUi("открыл приложение", `${app.title} · ${app.id}`);
    // Nothing to resolve, download or sign for a TestFlight title: the invite
    // is the whole delivery. Branch before the offer machinery runs.
    const invite = app.testflight?.url?.trim();
    if (invite) {
      setTestFlightApp(app);
      return;
    }
    const request = ++offerRequestRef.current;
    ++versionsRequestRef.current;
    setAppStoreVersions([]);
    setChosenVersion(null);
    setVersionsError("");
    setVersionsLoading(false);
    setVersionsVisibleCount(VERSIONS_PAGE);
    setRestoreVisibleCount(VERSIONS_PAGE);
    setVersionsSearch("");
    setRestoreVersionsSearch("");
    setDetailLoading(true);
    setLoadingSteps(OPEN_STEPS);
    setResolveError("");
    setOffer(null);
    setAppRestoreOpen(false);
    setAppRestoreScan(null);
    try {
      const nextOffer = await resolveOffer(app.id, false);
      if (request !== offerRequestRef.current) return;
      setOffer(nextOffer);
                                                                      
      setDetailLoading(false);
      void loadAppStoreVersions(nextOffer);
    } catch (e) {
      if (request === offerRequestRef.current) {
        setResolveError(errorMessage(e));
        setDetailLoading(false);
      }
    }
  }

  async function scanAppRestoreVersions(target: CatalogInstallOffer, force = false) {
    logUi(force ? "нажал «Обновить список»" : "нажал «Выбрать другую версию»", target.title);
    const appId = target.app_id;
    setAppRestoreOpen(true);
    setRestoreVisibleCount(VERSIONS_PAGE);
    setRestoreVersionsSearch("");
    setAppRestoreScan(null);
    setDetailLoading(true);
    setLoadingSteps(force ? RESTORE_REFRESH_STEPS : RESTORE_SCAN_STEPS);
    try {
      const scan = await command<AccountRestoreScan>("probe_account_restore_for_app", {
        input: {
          appId,
          force,
          currentAppStoreId: target.app_store_id ?? null,
          currentBundleId: target.bundle_id ?? null,
        },
      });
      setAppRestoreScan({
        ...scan,
        items: scan.items ?? [],
      });
    } catch (e) {
      setAppRestoreScan({
        items: [],
        message: errorMessage(e) || "Не удалось загрузить другие версии. Повторите попытку.",
      });
    } finally {
      setDetailLoading(false);
    }
  }

  async function loadAppStoreVersions(selected: CatalogInstallOffer, refresh = false) {
    logUi("запросил версии", `${selected.title} · ${selected.app_store_id ?? "без id"}`);
    const request = ++versionsRequestRef.current;
    if (!selected.app_store_id?.trim()) return;
    setVersionsLoading(true);
    setVersionsLoadingMore(false);
    setIosLoadingIds(new Set());
    setVersionsError("");
    setAppStoreVersions([]);
    setChosenVersion(null);
    setVersionsHasMore(false);
    setVersionsTotal(0);
    setVersionsVisibleCount(VERSIONS_PAGE);
    setVersionsSearch("");
    try {
      const page = await fetchAppStoreVersions(selected.app_store_id, { refresh });
      if (request !== versionsRequestRef.current) return;
      const versions = await namedAppStoreVersions(selected.app_store_id, page.versions || []);
      if (request !== versionsRequestRef.current) return;
      versionsOffsetRef.current = page.offset + (page.versions || []).length;
      setAppStoreVersions(versions);
      setVersionsHasMore(!!page.hasMore);
      setVersionsTotal(page.total || versions.length);
      setChosenVersion(versions.find(version => version.isLatest) ?? versions[0] ?? null);
      void loadVersionIos(selected.app_store_id, versions, request);
    } catch (e) {
      if (request === versionsRequestRef.current) setVersionsError(errorMessage(e));
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

  async function loadMoreAppStoreVersions(selected: CatalogInstallOffer) {
    if (versionsLoading || versionsLoadingMore || !versionsHasMore || !selected.app_store_id?.trim()) return;
    const request = versionsRequestRef.current;
    setVersionsLoadingMore(true);
    setVersionsError("");
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
    } catch (e) {
      if (request === versionsRequestRef.current) setVersionsError(errorMessage(e));
    } finally {
      if (request === versionsRequestRef.current) setVersionsLoadingMore(false);
    }
  }

  async function queueInstall(selected: CatalogInstallOffer) {
    logUi("нажал «Установить на iPhone»",
      `${selected.title} · версия ${chosenVersion?.version ?? selected.version} · источник ${selected.source}`);
    const versionConfirmedOnAccount = selected.source === "restore" || chosenVersion?.alreadyOwned === true;
    const temporaryUsRegion = selected.app_store_id === HAPP_APP_STORE_ID
      && !versionsLoading
      && !versionsError
      && appStoreVersions.length === 0;
    if (!temporaryUsRegion && !versionConfirmedOnAccount) {
      setVersionsError("На этом Apple ID нет подтверждённой версии для установки. Откройте другой Apple ID или обновите список версий.");
      return;
    }
    if (temporaryUsRegion && !window.confirm(
      "Happ пока нет в покупках этого Apple ID. Установка временно сменит страну магазина на США и сразу после установки вернёт вашу страну обратно.\n\nЕсли на счету Apple ID есть деньги, сменить страну не получится — проверьте баланс заранее.\n\nПродолжить?",
    )) return;
    const jobId = await command<string>("enqueue_catalog_install", { input: {
      title: selected.title,
      bundle_id: selected.bundle_id,
      app_store_id: selected.app_store_id,
      in_store: selected.in_store,
      min_ios: selected.min_ios,
      version: chosenVersion?.version ?? selected.version,
      external_version_id: chosenVersion?.externalVersionId,
      already_owned: versionConfirmedOnAccount,
      temporary_us_region: temporaryUsRegion,
    }});
                                                                                        
    const full = resolveAppActivation(apps.find(a => a.id === selected.app_id)?.activation);
    if (full) {
      const single = activationForInstalledVersion(full, {
        app_store_id: selected.app_store_id,
        bundle_id: selected.bundle_id,
        title: selected.title,
      });
      if (single) {
        storePendingPostInstallActivation({
          jobId,
          appTitle: selected.app_title,
          activation: single,
        });
      }
    }
    setOffer(null);
    navigate("/installs");
  }

  async function queueExport(selected: CatalogInstallOffer) {
    logUi("нажал «Скачать файл»",
      `${selected.title} · версия ${chosenVersion?.version ?? selected.version} · источник ${selected.source}`);
    const versionConfirmedOnAccount = selected.source === "restore" || chosenVersion?.alreadyOwned === true;
    if (!versionConfirmedOnAccount) {
      setVersionsError("На этом Apple ID нет подтверждённой версии для скачивания. Откройте другой Apple ID или обновите список версий.");
      return;
    }
    await command<string>("enqueue_catalog_export", { input: {
      title: selected.title,
      bundle_id: selected.bundle_id,
      app_store_id: selected.app_store_id,
      in_store: selected.in_store,
      min_ios: selected.min_ios,
      version: chosenVersion?.version ?? selected.version,
      external_version_id: chosenVersion?.externalVersionId,
      already_owned: versionConfirmedOnAccount,
    }});
    setExportQueued(true);
    sessionStorage.setItem("signum-export-queued", "1");
    setOffer(null);
    navigate("/installs");
  }

  async function queueOpenExportsFolder() {
    try {
      await command<string>("enqueue_open_exports_folder");
      navigate("/installs");
    } catch (e) {
      setResolveError(errorMessage(e));
    }
  }

  const offerVersionConfirmedOnAccount = !!offer
    && (offer.source === "restore" || chosenVersion?.alreadyOwned === true);
  const catalogActionNeedsOwnedVersion = !!offer
    && offer.app_store_id !== HAPP_APP_STORE_ID;
  const catalogActionDisabled = catalogActionNeedsOwnedVersion && !offerVersionConfirmedOnAccount;

  const catalogSubtitle = deviceIos
    ? `Банки и важные приложения · iPhone iOS ${deviceIos}`
    : "Банки и важные приложения для вашего iPhone";

  const filteredByGroup = useMemo(() => {
    if (groupFilter === "all") return apps;
    return apps.filter(app => (app.category || "app") === groupFilter);
  }, [apps, groupFilter]);

  const filteredByCompatibility = useMemo(() => {
    if (compatibilityFilter === "all" || !deviceIos) return filteredByGroup;
    return filteredByGroup.filter(app => {
      const compatible = !app.min_ios || compareIos(deviceIos, app.min_ios);
      return compatibilityFilter === "compatible" ? compatible : !compatible;
    });
  }, [compatibilityFilter, deviceIos, filteredByGroup]);

  const foundApps = useMemo(
    () => {
      const result = searchCatalogApps(filteredByCompatibility, searchQuery);
      if (catalogSort === "name") return [...result].sort((a, b) => a.title.localeCompare(b.title, "ru"));
      if (catalogSort === "ios") return [...result].sort((a, b) => {
        if (!a.min_ios) return b.min_ios ? -1 : 0;
        if (!b.min_ios) return 1;
        return a.min_ios.localeCompare(b.min_ios, undefined, { numeric: true });
      });
      return result;
    },
    [catalogSort, filteredByCompatibility, searchQuery],
  );

  // The walkthrough puts its own card first so the very first thing the user
  // sees is the thing being explained. It exists only while the tour runs.
  const visibleApps = useMemo(
    () => (tourDemo ? [tourCatalogApp, ...foundApps] : foundApps),
    [foundApps, tourDemo],
  );

  const favoriteApps = useMemo(
    () => (searchQuery.trim() || catalogSort !== "default" || compatibilityFilter !== "all"
      ? []
      : apps.filter(app => favorites.includes(app.id))),
    [apps, catalogSort, compatibilityFilter, favorites, searchQuery],
  );

                                                                                                            
  const searchBankAlt = useMemo(
    () => (searchQuery.trim() ? detectBankKey(searchQuery) : null),
    [searchQuery],
  );

  const offerBankAlt = useMemo(() => {
    if (!offer) return null;
    return detectBankFromApp({
      id: offer.app_id,
      title: offer.app_title || offer.title,
      aliases: [],
    }) ?? detectBankKey(`${offer.app_id} ${offer.app_title} ${offer.title}`);
  }, [offer]);

  const appsByGroup = useMemo(() => {
    if (groupFilter !== "all" || searchQuery.trim() || compatibilityFilter !== "all" || catalogSort !== "default") return null;
    const map = new Map<string, CatalogApp[]>();
    for (const app of apps) {
      const key = app.category || "app";
      const list = map.get(key) ?? [];
      list.push(app);
      map.set(key, list);
    }
    const ordered = [...groups].sort((a, b) => (b.priority ?? 0) - (a.priority ?? 0));
    const used = new Set(ordered.map(g => g.id));
    const sections: Array<{ id: string; title: string; apps: CatalogApp[] }> = [];
    for (const group of ordered) {
      const items = map.get(group.id);
      if (items?.length) sections.push({ id: group.id, title: group.title, apps: items });
    }
    for (const [id, items] of map) {
      if (used.has(id) || !items.length) continue;
      const fallbackTitles: Record<string, string> = {
        sber: "Сбер",
        tbank: "Т-Банк",
        alfa: "Альфа",
        gazprom: "Газпромбанк",
        novikom: "Новикомбанк",
        bank: "Банки",
        investment: "Инвестиции",
        app: "Другие",
        other: "Другие",
      };
      sections.push({ id, title: fallbackTitles[id] || id, apps: items });
    }
    // Во время обучения демо-карточка идёт первой секцией — на неё указывает подсказка.
    if (tourDemo) sections.unshift({ id: "tour-demo", title: "Пример", apps: [tourCatalogApp] });
    return sections;
  }, [apps, groups, groupFilter, searchQuery, tourDemo, compatibilityFilter, catalogSort]);

  function bannerStyle(style?: string): string {
    switch ((style || "blue").toLowerCase()) {
      case "amber":
        return "bg-amber-gradient";
      case "violet":
        return "from-violet-600 to-indigo-700 bg-gradient-to-br";
      default:
        return "bg-blue-gradient";
    }
  }

  return (
    <div className="mx-auto max-w-[1500px] p-7 lg:p-9">
      <PageHeader title="Каталог" subtitle={catalogSubtitle} actions={
        <div className="flex flex-wrap gap-2">
          {selectMode ? (
            <>
              <button
                className="btn-primary"
                disabled={selectedIds.size === 0 || bulkBusy}
                onClick={() => void installSelected()}
              >
                {bulkBusy ? <LoaderCircle size={15} className="animate-spin" /> : <Download size={15} />}
                Установить выбранные{selectedIds.size > 0 ? ` (${selectedIds.size})` : ""}
              </button>
              <button className="btn-secondary" onClick={() => { setSelectMode(false); setSelectedIds(new Set()); }}>
                Отмена
              </button>
            </>
          ) : (
            <>
              <button className="btn-secondary" onClick={() => setSelectMode(true)}>
                <ListChecks size={15} />Выбрать несколько
              </button>
              <button className="btn-secondary" onClick={() => void load()}><RefreshCw size={15} />Обновить</button>
            </>
          )}
        </div>
      } />



      {offlineSince !== null && <p role="status" className="mt-4 rounded-xl border border-amber-400/25 bg-amber-400/10 p-4 text-sm text-amber-200">
        Не удалось обновить каталог. Показана сохранённая копия{offlineSince > 0 ? ` от ${new Date(offlineSince * 1000).toLocaleString("ru-RU")}` : ""}. Нажмите «Обновить», чтобы проверить соединение.
      </p>}

      <div className="mt-6 space-y-3">
        {banners.map(banner => (
          <div
            key={banner.id}
            className={`relative overflow-hidden rounded-2xl p-5 text-white shadow-panel ${bannerStyle(banner.style)}`}
          >
            <div className="relative flex flex-wrap items-center justify-between gap-4">
              <div className="min-w-0 max-w-3xl">
                <h2 className="text-base font-bold sm:text-lg">{banner.title}</h2>
                {banner.text?.trim() && (
                  <p className="mt-1.5 text-xs leading-5 text-white/80 sm:text-sm">{banner.text}</p>
                )}
              </div>
              {banner.app_id && (
                <button
                  type="button"
                  className="shrink-0 rounded-xl bg-white px-4 py-2.5 text-xs font-bold text-blue-700 shadow-lg transition hover:scale-[1.02]"
                  onClick={() => {
                    const app = apps.find(a => a.id === banner.app_id);
                    if (app) void openInstallOffer(app);
                    else setResolveError("Приложение из баннера пока не найдено в каталоге. Нажмите «Обновить».");
                  }}
                >
                  <Smartphone className="mr-2 inline" size={15} />
                  {banner.action_label?.trim() || "Установить на iPhone"}
                </button>
              )}
            </div>
          </div>
        ))}
      </div>

      {tourOffer && !tourDemo && (
        <section className="panel mt-7 flex flex-wrap items-center justify-between gap-4 border-brand-400/25 p-5">
          <div className="flex min-w-0 items-start gap-3">
            <div className="shrink-0 rounded-xl bg-brand-500/15 p-2.5 text-brand-300">
              <GraduationCap size={20} />
            </div>
            <div className="min-w-0">
              <div className="text-sm font-bold text-theme">Не разобрались? Пройдите обучение</div>
              <p className="mt-1 text-xs leading-5 text-slate-500">
                За пару минут покажем, как найти нужное приложение и поставить его на iPhone.
                Ничего менять не придётся — просто пройдём по шагам.
              </p>
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <button
              type="button"
              className="btn-primary"
              onClick={() => {
                hideTourOffer();
                resetOnboarding();
                window.location.reload();
              }}
            >
              <GraduationCap size={15} />
              Пройти обучение
            </button>
            <button
              type="button"
              className="btn-secondary px-3"
              aria-label="Закрыть предложение"
              title="Больше не показывать"
              onClick={() => {
                hideTourOffer();
                setTourOffer(false);
              }}
            >
              <X size={16} />
            </button>
          </div>
        </section>
      )}

      <div className="mt-7">
        <div className="panel flex items-center gap-3 px-4 py-3 transition focus-within:border-brand-400/40 focus-within:bg-white/[0.055]">
          <Search className="shrink-0 text-brand-300" size={19} />
          <input
            type="text"
            value={searchQuery}
            onChange={event => setSearchQuery(event.target.value)}
            placeholder="Поиск"
            aria-label="Поиск приложений"
            className="min-w-0 flex-1 bg-transparent text-sm text-white outline-none placeholder:text-slate-600"
          />
          {searchQuery && (
            <button
              type="button"
              onClick={() => setSearchQuery("")}
              className="rounded-lg p-1.5 text-slate-500 transition hover:bg-white/[0.06] hover:text-white"
              aria-label="Очистить поиск"
            >
              <X size={16} />
            </button>
          )}
        </div>
        {groups.length > 0 && (
          <div className="mt-3 flex flex-wrap gap-2">
            <button
              type="button"
              onClick={() => setGroupFilter("all")}
              className={`rounded-xl px-3.5 py-2 text-xs font-semibold transition ${
                groupFilter === "all"
                  ? "bg-brand-500/20 text-brand-200 ring-1 ring-brand-400/30"
                  : "bg-white/[0.04] text-slate-400 hover:bg-white/[0.07] hover:text-white"
              }`}
            >
              Все
            </button>
            {groups.map(group => (
              <button
                key={group.id}
                type="button"
                onClick={() => setGroupFilter(group.id)}
                className={`rounded-xl px-3.5 py-2 text-xs font-semibold transition ${
                  groupFilter === group.id
                    ? "bg-brand-500/20 text-brand-200 ring-1 ring-brand-400/30"
                    : "bg-white/[0.04] text-slate-400 hover:bg-white/[0.07] hover:text-white"
                }`}
              >
                {group.title}
              </button>
            ))}
          </div>
        )}
        <div className="mt-3 flex flex-wrap items-center gap-2">
          <span className="text-[11px] font-semibold text-slate-600">Сортировка</span>
          <details className="catalog-sort-picker">
            <summary aria-label={`Сортировка: ${{ default: "По каталогу", name: "По названию", ios: "По минимальной iOS" }[catalogSort]}`}>
              <span>{({ default: "По каталогу", name: "По названию", ios: "По минимальной iOS" } as const)[catalogSort]}</span>
              <ChevronDown size={14} aria-hidden="true" />
            </summary>
            <div className="catalog-sort-menu" role="group" aria-label="Варианты сортировки">
              {([["default", "По каталогу"], ["name", "По названию"], ["ios", "По минимальной iOS"]] as const).map(([value, label]) => (
                <button
                  key={value}
                  type="button"
                  className={catalogSort === value ? "is-selected" : ""}
                  aria-pressed={catalogSort === value}
                  onClick={event => {
                    setCatalogSort(value);
                    event.currentTarget.closest("details")?.removeAttribute("open");
                  }}
                >
                  <span>{label}</span>
                  {catalogSort === value && <Check size={15} aria-hidden="true" />}
                </button>
              ))}
            </div>
          </details>
          {deviceIos && (
            <>
              <button type="button" onClick={() => setCompatibilityFilter("all")} className={`rounded-xl px-3 py-2 text-xs font-semibold ${compatibilityFilter === "all" ? "bg-brand-500/20 text-brand-200 ring-1 ring-brand-400/30" : "bg-white/[0.04] text-slate-500 hover:text-white"}`}>Все</button>
              <button type="button" onClick={() => setCompatibilityFilter("compatible")} className={`rounded-xl px-3 py-2 text-xs font-semibold ${compatibilityFilter === "compatible" ? "bg-emerald-400/15 text-emerald-200 ring-1 ring-emerald-400/30" : "bg-white/[0.04] text-slate-500 hover:text-white"}`}>Подходит для iOS {deviceIos}</button>
              <button type="button" onClick={() => setCompatibilityFilter("incompatible")} className={`rounded-xl px-3 py-2 text-xs font-semibold ${compatibilityFilter === "incompatible" ? "bg-amber-400/15 text-amber-200 ring-1 ring-amber-400/30" : "bg-white/[0.04] text-slate-500 hover:text-white"}`}>Не подходит</button>
            </>
          )}
        </div>
        {searchQuery.trim() && (
          <div className="mt-2 px-1 text-right text-[11px] text-slate-600">Найдено: {visibleApps.length}</div>
        )}
      </div>

      <div className="mt-8">
        {loading && !tourDemo ? <StatusMessage loading>Загрузка каталога…</StatusMessage> : error && !tourDemo ? <StatusMessage error={error} /> : apps.length === 0 && !tourDemo ? <StatusMessage error="Каталог пока пуст" /> :
          visibleApps.length === 0 ? (
            <div className="panel px-6 py-10 text-center">
              <Search className="mx-auto text-slate-600" size={28} />
              <p className="mt-3 text-sm font-semibold text-slate-300">Ничего не найдено</p>
              <p className="mt-1 text-xs text-slate-600">Попробуйте полное название, сокращение или английский вариант.</p>
              <div className="mx-auto mt-6 max-w-sm rounded-2xl border border-white/[0.08] bg-black/15 p-4">
                <p className="text-xs leading-5 text-slate-400">
                  В каталоге лежат только банки и сервисы, которых нет в App Store.
                  Любое другое приложение ищите на странице «Поиск» — оттуда ставится всё,
                  что доступно вашему Apple ID.
                </p>
                <button
                  type="button"
                  className="btn-secondary mt-3 w-full"
                  onClick={() => navigate("/find", { state: { query: searchQuery.trim() } })}
                >
                  <Search size={15} />
                  {searchQuery.trim() ? `Искать «${searchQuery.trim()}» в App Store` : "Открыть поиск по App Store"}
                </button>
              </div>
              {searchBankAlt && (
                <div className="mx-auto mt-6 max-w-sm rounded-2xl border border-brand-400/25 bg-brand-500/10 p-4 text-left">
                  <div className="flex items-center gap-2.5 text-sm font-semibold text-brand-100">
                    <img
                      src={bankIconUrl(searchBankAlt)}
                      alt={bankDisplayName(searchBankAlt)}
                      className="h-8 w-8 rounded-xl object-cover ring-1 ring-white/10"
                      referrerPolicy="no-referrer"
                    />
                    {bankDisplayName(searchBankAlt)}
                  </div>
                  <p className="mt-2 text-xs leading-5 text-slate-400">
                    В каталоге нет подходящей версии. Можно установить другим способом — по номеру телефона.
                  </p>
                  <button
                    type="button"
                    className="btn-primary mt-4 w-full"
                    onClick={() => setBankAlt(searchBankAlt)}
                  >
                    Альтернативный способ установки
                  </button>
                </div>
              )}
            </div>
          ) : (
            <>
            {favoriteApps.length > 0 && (
              <section className="mb-10">
                <div className="mb-4 flex items-end justify-between gap-3">
                  <h2 className="flex items-center gap-2 text-lg font-bold tracking-tight text-white">
                    <Star size={16} className="fill-amber-300 text-amber-300" />Избранное
                  </h2>
                  <span className="text-[11px] text-slate-600">{favoriteApps.length}</span>
                </div>
                <div className="grid grid-cols-[repeat(auto-fill,minmax(230px,1fr))] gap-4">
                  {favoriteApps.map(app => (
                    <AppCard
                      key={`fav-${app.id}`}
                      app={app}
                      favorite
                      onToggleFavorite={onCardFavorite}
                      selectable={selectMode}
                      selected={selectedIds.has(app.id)}
                      onClick={onCardClick}
                    />
                  ))}
                </div>
              </section>
            )}
            {appsByGroup ? (
            <div className="space-y-10">
              {appsByGroup.map(section => (
                <section key={section.id}>
                  <div className="mb-4 flex items-end justify-between gap-3">
                    <h2 className="text-lg font-bold tracking-tight text-white">{section.title}</h2>
                    <span className="text-[11px] text-slate-600">{section.apps.length}</span>
                  </div>
                  <div className="grid grid-cols-[repeat(auto-fill,minmax(230px,1fr))] gap-4">
                    {section.apps.map(app => (
                      <AppCard
                        key={app.id}
                        app={app}
                        dataTour={app.id === TOUR_APP_ID ? "catalog-demo-card" : undefined}
                        favorite={favorites.includes(app.id)}
                        onToggleFavorite={onCardFavorite}
                        selectable={selectMode}
                        selected={selectedIds.has(app.id)}
                        onClick={onCardClick}
                      />
                    ))}
                  </div>
                </section>
              ))}
            </div>
          ) : (
            <div className="grid grid-cols-[repeat(auto-fill,minmax(230px,1fr))] gap-4">
              {visibleApps.map(app => (
                <AppCard
                  key={app.id}
                  app={app}
                  dataTour={app.id === TOUR_APP_ID ? "catalog-demo-card" : undefined}
                  favorite={favorites.includes(app.id)}
                  onToggleFavorite={onCardFavorite}
                  selectable={selectMode}
                  selected={selectedIds.has(app.id)}
                  onClick={onCardClick}
                />
              ))}
            </div>
            )}
            </>
          )}
      </div>

      {detailLoading && !appRestoreOpen && (
        <div className="fixed inset-0 z-50 grid place-items-center bg-black/60 backdrop-blur-sm">
          <div className="panel flex items-center gap-3 px-5 py-4">
            <LoaderCircle className="animate-spin text-brand-300" size={20} />
            <span className="text-sm text-slate-300"><LoadingSteps steps={loadingSteps} /></span>
          </div>
        </div>
      )}

      {resolveError && !detailLoading && (
        <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-6 backdrop-blur-md" onMouseDown={() => setResolveError("")}>
          <div className="panel w-full max-w-md p-6" onMouseDown={e => e.stopPropagation()}>
            <div className="flex items-start justify-between gap-4">
              <h2 className="text-lg font-bold text-rose-200">Не удалось подобрать версию</h2>
              <button className="rounded-lg p-2 text-slate-500 hover:bg-white/5 hover:text-white" onClick={() => setResolveError("")}><X size={18} /></button>
            </div>
            <p className="mt-4 text-sm leading-6 text-slate-400">{resolveError}</p>
            <button className="btn-secondary mt-5 w-full" onClick={() => setResolveError("")}>Закрыть</button>
          </div>
        </div>
      )}

      {appRestoreOpen && offer && (
        <div
          className="fixed inset-0 z-[60] grid place-items-center bg-black/70 p-6 backdrop-blur-md"
          onMouseDown={() => { if (!detailLoading) setAppRestoreOpen(false); }}
        >
          <div className="panel flex max-h-[85vh] w-full max-w-2xl flex-col p-6" onMouseDown={e => e.stopPropagation()}>
            <div className="flex items-start justify-between gap-4">
              <div>
                <h2 className="text-xl font-bold">Другие версии · {offer.app_title}</h2>
                <p className="mt-1 text-xs text-slate-500">
                  {detailLoading
                    ? <LoadingSteps steps={loadingSteps} />
                    : appRestoreScan?.device_ios
                      ? <>Только версии из покупок этого Apple ID · iPhone iOS {appRestoreScan.device_ios}</>
                      : <>Только версии из покупок этого Apple ID. Подключите iPhone, чтобы проверить совместимость.</>}
                </p>
                {!detailLoading && (
                  <button
                    className="mt-1 text-[11px] text-brand-300 hover:text-white flex items-center gap-1"
                    onClick={() => void scanAppRestoreVersions(offer, true)}
                  >
                    <RefreshCw size={12} /> Обновить список
                  </button>
                )}
              </div>
              {!detailLoading && (
                <button className="rounded-lg p-2 text-slate-500 hover:bg-white/5 hover:text-white" onClick={() => setAppRestoreOpen(false)}><X size={18} /></button>
              )}
            </div>

            {detailLoading ? (
              <div className="mt-8 flex flex-col items-center justify-center gap-4 py-10">
                <LoaderCircle className="animate-spin text-brand-300" size={32} />
                <p className="max-w-sm text-center text-sm leading-6 text-slate-400"><LoadingSteps steps={loadingSteps} /></p>
              </div>
            ) : appRestoreScan && (
              <div className="mt-5 flex min-h-0 flex-1 flex-col">
                {(appRestoreScan.items ?? []).length === 0 ? (
                  <div className="rounded-xl border border-white/[0.06] bg-black/10 p-6 text-center text-sm text-slate-400">
                    {appRestoreScan.message?.trim()
                      || "Других версий этого приложения в покупках Apple ID не найдено."}
                  </div>
                ) : (
                  <>
                    {appRestoreScan.message?.trim() && (
                      <p className="mb-2 rounded-lg border border-amber-400/25 bg-amber-400/10 p-2.5 text-[11px] leading-5 text-amber-100/80">
                        {appRestoreScan.message}
                      </p>
                    )}
                    <label className="relative mb-2 block">
                      <Search size={13} className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-slate-500" />
                      <input
                        type="search"
                        className="field w-full py-2 pl-8 pr-8 text-xs"
                        placeholder="Поиск"
                        value={restoreVersionsSearch}
                        onChange={e => {
                          setRestoreVersionsSearch(e.target.value);
                          setRestoreVisibleCount(VERSIONS_PAGE);
                        }}
                      />
                      {restoreVersionsSearch && (
                        <button
                          type="button"
                          className="absolute right-2 top-1/2 -translate-y-1/2 p-0.5 text-slate-500 hover:text-white"
                          onClick={() => {
                            setRestoreVersionsSearch("");
                            setRestoreVisibleCount(VERSIONS_PAGE);
                          }}
                          aria-label="Очистить"
                        >
                          <X size={13} />
                        </button>
                      )}
                    </label>
                    <div className="mb-2 text-[11px] text-slate-500">
                      {filteredRestoreVersions.length === 0
                        ? "0"
                        : `${Math.min(restoreVisibleCount, filteredRestoreVersions.length)} из ${filteredRestoreVersions.length}`}
                      {restoreVersionsSearch.trim() && (appRestoreScan.items ?? []).length !== filteredRestoreVersions.length
                        ? ` (из ${(appRestoreScan.items ?? []).length})`
                        : filteredRestoreVersions.length === (appRestoreScan.items ?? []).length
                          ? ""
                          : ""}
                    </div>
                    {filteredRestoreVersions.length === 0 ? (
                      <div className="rounded-xl border border-white/[0.06] bg-black/10 p-4 text-center text-sm text-slate-400">
                        Ничего не найдено по «{restoreVersionsSearch.trim()}».
                      </div>
                    ) : (
                    <div className="min-h-0 flex-1 space-y-2 overflow-y-auto overscroll-contain pr-1">
                      {filteredRestoreVersions.slice(0, restoreVisibleCount).map(item => (
                        <button
                          key={`${item.app_id}-${item.app_store_id ?? item.bundle_id}-${item.version}`}
                          className="panel flex w-full items-center gap-4 p-4 text-left transition hover:border-white/[0.12] hover:bg-white/[0.04]"
                          onClick={() => {
                            ++offerRequestRef.current;
                            const next = restoreItemToOffer(item, appRestoreScan.device_ios ?? deviceIos);
                            setAppRestoreOpen(false);
                            setOffer(next);
                            void loadAppStoreVersions(next);
                          }}
                        >
                          {item.icon_url ? (
                            <img key={item.icon_url} className="h-12 w-12 rounded-xl object-cover" src={item.icon_url} alt="" />
                          ) : (
                            <div className="grid h-12 w-12 place-items-center rounded-xl bg-brand-gradient text-lg font-black">{(item.app_title || item.title || "?").slice(0, 1)}</div>
                          )}
                          <div className="min-w-0 flex-1">
                            <div className="font-semibold text-slate-100">{item.title}</div>
                            <div className="mt-1 text-xs text-brand-200">{formatVersionWithIos(item.version, item.min_ios)}</div>
                            {item.description?.trim() && (
                              <div className="mt-1 text-xs text-slate-500">{item.description.trim()}</div>
                            )}
                            {item.installed_on_device && (
                              <div className="mt-1 text-[11px] text-sky-300">
                                Уже на iPhone{item.installed_version ? ` · v${item.installed_version}` : ""}
                              </div>
                            )}
                            {item.device_compatible === false && (
                              <div className="mt-1 text-[11px] text-amber-300">
                                Не запустится на iOS {appRestoreScan.device_ios ?? deviceIos ?? "устройства"}
                              </div>
                            )}
                          </div>
                          <div className="flex shrink-0 flex-col items-end gap-1">
                            {item.in_store && <span className="rounded-lg bg-emerald-400/10 px-2 py-1 text-[10px] font-bold uppercase text-emerald-300">App Store</span>}
                            {!item.in_store && <span className="rounded-lg bg-slate-400/10 px-2 py-1 text-[10px] font-bold uppercase text-slate-400">Не в Store</span>}
                            {item.installed_on_device && <span className="rounded-lg bg-sky-400/10 px-2 py-1 text-[10px] font-bold uppercase text-sky-300">Установлено</span>}
                            {item.device_compatible === true && <span className="rounded-lg bg-emerald-400/10 px-2 py-1 text-[10px] font-bold uppercase text-emerald-300">Подходит</span>}
                            {item.device_compatible === false && <span className="rounded-lg bg-amber-400/10 px-2 py-1 text-[10px] font-bold uppercase text-amber-300">Не запустится</span>}
                            {item.device_compatible == null && appRestoreScan.device_ios && <span className="text-[10px] text-slate-600">—</span>}
                          </div>
                        </button>
                      ))}
                    </div>
                    )}
                    {restoreVisibleCount < filteredRestoreVersions.length && (
                      <button
                        type="button"
                        className="btn-secondary mt-3 w-full text-xs"
                        onClick={() => setRestoreVisibleCount(n => n + VERSIONS_PAGE)}
                      >
                        Ещё {Math.min(VERSIONS_PAGE, filteredRestoreVersions.length - restoreVisibleCount)} из{" "}
                        {filteredRestoreVersions.length - restoreVisibleCount}
                      </button>
                    )}
                  </>
                )}
              </div>
            )}
            {!detailLoading && (
              <button className="btn-secondary mt-4 w-full" onClick={() => setAppRestoreOpen(false)}>Назад</button>
            )}
          </div>
        </div>
      )}

      {testFlightApp?.testflight?.url && (

        <TestFlightModal

          app={testFlightApp}

          inviteUrl={testFlightApp.testflight.url}

          onClose={() => setTestFlightApp(null)}

        />

      )}


      {offer && (
        <div className="fixed inset-0 z-50 flex items-center justify-center overflow-y-auto bg-black/70 p-6 backdrop-blur-md" onMouseDown={() => { setOffer(null); setResolveError(""); }}>
          <div data-tour="install-sheet" className="panel max-h-[calc(100vh-3rem)] w-full max-w-lg overflow-y-auto overscroll-contain p-6" onMouseDown={e => e.stopPropagation()}>
            <div className="flex items-start justify-between gap-4">
              <div className="flex items-center gap-4">
                {offer.icon_url ? (
                  <img key={offer.icon_url} className="h-16 w-16 rounded-2xl object-cover drop-shadow-icon-lg" src={offer.icon_url} alt="" />
                ) : (
                  <div className="grid h-16 w-16 place-items-center rounded-2xl bg-brand-gradient text-2xl font-black">{offer.title.slice(0, 1)}</div>
                )}
                <div>
                  <h2 className="text-xl font-bold">{offer.app_title}</h2>
                  <p className="mt-1 text-xs text-slate-500">{offer.title} · {formatVersionWithIos(chosenVersion?.version ?? offer.version, offer.min_ios)}</p>
                  {(offer.device_ios || deviceIos) && offer.min_ios && (
                    <p className={`mt-1 flex items-center gap-1 text-[11px] ${
                      offer.device_compatible === false || !compareIos(offer.device_ios || deviceIos || "", offer.min_ios)
                        ? "text-amber-300"
                        : "text-slate-600"
                    }`}>
                      <Smartphone size={12} />
                      iPhone iOS {offer.device_ios || deviceIos}
                      {offer.device_compatible === false || !compareIos(offer.device_ios || deviceIos || "", offer.min_ios)
                        ? " · не поддерживает эту версию"
                        : " · совместимо"}
                    </p>
                  )}
                </div>
              </div>
              <button className="rounded-lg p-2 text-slate-500 hover:bg-white/5 hover:text-white" onClick={() => { setOffer(null); setResolveError(""); }}><X size={18} /></button>
            </div>

            {offer.installed_on_device && (
              <div className="mt-4 flex items-start gap-3 rounded-xl border border-sky-400/20 bg-sky-400/10 p-4">
                <Smartphone size={18} className="mt-0.5 shrink-0 text-sky-300" />
                <div>
                  <div className="text-sm font-semibold text-sky-100">Уже установлено на iPhone</div>
                  <p className="mt-1 text-xs leading-5 text-sky-200/80">
                    На устройстве{offer.installed_version ? ` версия ${offer.installed_version}` : " уже есть это приложение"}.
                    Установить можно снова — будет переустановка.
                  </p>
                </div>
              </div>
            )}

            {(offer.device_compatible === false || (deviceIos && offer.min_ios && !compareIos(deviceIos, offer.min_ios))) && (
              <div className="mt-4 flex items-start gap-3 rounded-xl border border-amber-400/25 bg-amber-400/10 p-4">
                <AlertTriangle size={18} className="mt-0.5 shrink-0 text-amber-300" />
                <div>
                  <div className="text-sm font-semibold text-amber-100">iPhone не поддерживает это приложение</div>
                  <p className="mt-1 text-xs leading-5 text-amber-100/80">
                    Нужна iOS {offer.min_ios}
                    {(offer.device_ios || deviceIos) ? `, у вас ${offer.device_ios || deviceIos}` : ""}.
                    Установить можно, но приложение на этом телефоне не запустится.
                  </p>
                </div>
              </div>
            )}

            <div className={`mt-5 flex items-start gap-3 rounded-xl border p-4 ${
              offer.source === "app_store"
                ? "border-emerald-400/20 bg-emerald-400/10"
                : "border-amber-400/20 bg-amber-400/10"
            }`}>
              <div className="mt-0.5 text-brand-300">
                {offer.source === "app_store" ? <Store size={18} /> : <RotateCcw size={18} />}
              </div>
              <div>
                <div className="text-sm font-semibold text-slate-100">
                  {offer.source === "app_store" ? "App Store" : "Восстановление из покупок"}
                </div>
                <p className="mt-1 text-xs leading-5 text-slate-400">{offer.message}</p>
                {offer.description?.trim() && (
                  <p className="mt-2 text-xs leading-5 text-slate-500">{offer.description.trim()}</p>
                )}
                {formatMinIos(offer.min_ios) && <p className="mt-2 text-xs text-slate-500">Минимальная система: {formatMinIos(offer.min_ios)}</p>}
              </div>
            </div>

            <div data-tour="sheet-versions" className="mt-5 rounded-xl border border-white/[0.08] p-3">
              <div className="flex items-center justify-between gap-3">
                <span className="text-xs font-semibold text-slate-200">
                  Версия для скачивания
                  {!versionsLoading && appStoreVersions.length > 0 && (
                    <span className="ml-1.5 font-normal text-slate-500">
                      {versionsSearch.trim()
                        ? `${filteredAppStoreVersions.length} из ${appStoreVersions.length}`
                        : versionsTotal > appStoreVersions.length
                          ? `${appStoreVersions.length} из ${versionsTotal}`
                          : `${appStoreVersions.length}`}
                    </span>
                  )}
                </span>
                <span className="flex items-center gap-3">
                  {deviceIos && offer.app_store_id && (
                    <button
                      className="text-[11px] text-brand-300 hover:text-white disabled:opacity-50"
                      disabled={fitBusy || versionsLoading}
                      onClick={() => void pickVersionForDevice(offer)}
                    >
                      {fitBusy ? "Подбираем…" : "Подобрать под мой iPhone"}
                    </button>
                  )}
                  <button
                    className="text-[11px] text-brand-300 hover:text-white disabled:opacity-50"
                    disabled={versionsLoading || !offer.app_store_id}
                    onClick={() => void loadAppStoreVersions(offer, true)}
                  >
                    Обновить
                  </button>
                </span>
              </div>
              {versionsLoading ? (
                <div className="mt-3 flex items-center gap-2 text-xs text-slate-500">
                  <LoaderCircle size={14} className="animate-spin text-brand-300" />
                  Загружаем список версий…
                </div>
              ) : versionsError ? (
                <div className="mt-2 space-y-2">
                  <p className="text-xs leading-5 text-rose-300 whitespace-pre-line">{versionsError}</p>
                  {(versionsError.toLowerCase().includes("войдите")
                    || versionsError.toLowerCase().includes("аккаунт")
                    || versionsError.toLowerCase().includes("сессия")
                    || versionsError.toLowerCase().includes("token")) && (
                    <button
                      type="button"
                      className="text-[11px] font-medium text-brand-300 hover:text-white"
                      onClick={() => navigate("/account")}
                    >
                      Открыть «Аккаунт»
                    </button>
                  )}
                </div>
              ) : appStoreVersions.length === 0 ? (
                offer.app_store_id === HAPP_APP_STORE_ID ? (
                  <div className="mt-2 rounded-lg border border-amber-400/25 bg-amber-400/10 p-3 text-xs leading-5 text-amber-100/80">
                    Happ отсутствует в покупках этого Apple ID. При установке Signum временно сменит страну магазина на США, добавит Happ и вернёт исходную страну. Смена не пройдёт, если на счету Apple ID есть деньги.
                  </div>
                ) : offer.source === "restore" ? (
                  // Версия подтверждена покупками — та же проверка, что пускает
                  // кнопки установки и скачивания. Говорить здесь «недоступно»
                  // значит пугать человека там, где всё работает.
                  <p className="mt-2 text-xs text-slate-500">
                    Полный список версий сейчас недоступен — Apple отдаёт его не для всех приложений.
                    Установить и скачать можно версию {offer.version} из покупок Apple ID.
                  </p>
                ) : (
                  <p className="mt-2 text-xs text-amber-200/80">
                    На этом Apple ID нет подтверждённой версии. Установка и скачивание недоступны.
                    Если приложение было приобретено на другом Apple ID — войдите в него и обновите список.
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
                      onChange={e => {
                        setVersionsSearch(e.target.value);
                        setVersionsVisibleCount(VERSIONS_PAGE);
                      }}
                    />
                    {versionsSearch && (
                      <button
                        type="button"
                        className="absolute right-2 top-1/2 -translate-y-1/2 p-0.5 text-slate-500 hover:text-white"
                        onClick={() => {
                          setVersionsSearch("");
                          setVersionsVisibleCount(VERSIONS_PAGE);
                        }}
                        aria-label="Очистить"
                      >
                        <X size={13} />
                      </button>
                    )}
                  </label>
                  {filteredAppStoreVersions.length === 0 ? (
                    <p className="mt-2 text-xs text-slate-500">Ничего не найдено по «{versionsSearch.trim()}».</p>
                  ) : (
                    <>
                      <div className="mt-2 max-h-52 space-y-1 overflow-y-auto overscroll-contain pr-0.5">
                        {filteredAppStoreVersions.slice(0, versionsVisibleCount).map(version => (
                          <label
                            key={version.externalVersionId}
                            className="flex cursor-pointer items-center justify-between gap-3 rounded-lg px-2 py-1.5 text-xs hover:bg-white/[0.05]"
                          >
                            <span className="flex min-w-0 items-center gap-2">
                              <input
                                type="radio"
                                name="app-store-version"
                                checked={chosenVersion?.externalVersionId === version.externalVersionId}
                                onChange={() => setChosenVersion(version)}
                              />
                              <span className="min-w-0 truncate">
                                {formatAppStoreVersionLabel(version.version, version.externalVersionId)}
                                {/* Безымянной сборке номер уже вписан в ярлык; второй раз он только
                                    переполнял строку, и truncate срезал его до голого «сборка». */}
                                {!formatAppStoreVersionLabel(version.version, version.externalVersionId).includes(version.externalVersionId) && (
                                  <span className="ml-1.5 text-[10px] text-slate-400">{version.externalVersionId}</span>
                                )}
                              </span>
                              {version.isLatest && <span className="shrink-0 text-brand-300">Последняя</span>}
                            </span>
                            <span className="flex shrink-0 items-center gap-2">
                              {version.minIos && (
                                deviceIosUi && !compareIos(deviceIosUi, version.minIos)
                                  ? <span className="text-rose-300">Не запустится · {formatMinIos(version.minIos)}</span>
                                  : <span className="text-slate-500">{formatMinIos(version.minIos)}</span>
                              )}
                              {!version.minIos && iosLoadingIds.has(version.externalVersionId) && (
                                <span className="inline-flex items-center gap-1 text-slate-500" role="status">
                                  <LoaderCircle size={12} className="animate-spin" />iOS…
                                </span>
                              )}
                            </span>
                          </label>
                        ))}
                      </div>
                      {versionsVisibleCount < filteredAppStoreVersions.length && (
                        <button
                          type="button"
                          className="mt-2 w-full rounded-lg border border-white/[0.08] py-2 text-[11px] font-semibold text-brand-300 transition hover:bg-white/[0.04] hover:text-white"
                          onClick={() => setVersionsVisibleCount(n => n + VERSIONS_PAGE)}
                        >
                          Показать ещё в списке
                        </button>
                      )}
                      {versionsHasMore && !versionsSearch.trim() && (
                        <button
                          type="button"
                          className="mt-2 w-full rounded-lg border border-white/[0.08] py-2 text-[11px] font-semibold text-brand-300 transition hover:bg-white/[0.04] hover:text-white disabled:opacity-50"
                          disabled={versionsLoadingMore}
                          onClick={() => void loadMoreAppStoreVersions(offer)}
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
                </>
              )}
            </div>

            <div className="mt-5 space-y-2">
              <button
                data-tour="sheet-install"
                className="btn-primary w-full"
                disabled={catalogActionDisabled || versionsLoading}
                onClick={() => void queueInstall(offer)}
              >
                <Smartphone size={15} />
                {versionsLoading
                  ? "Загружаем версии…"
                  : catalogActionDisabled
                    ? "Нет версии в покупках"
                    : offer.installed_on_device
                      ? "Переустановить на iPhone"
                      : "Установить на iPhone"}
              </button>
              <button
                data-tour="sheet-export"
                className="btn-secondary w-full"
                /* Пока версии не загружены, скачивать нечего: подтверждённой
                   версии ещё нет, и задача повисала бы в очереди до конца
                   опроса — со стороны это выглядело зависанием. */
                disabled={catalogActionDisabled || versionsLoading}
                onClick={() => void queueExport(offer)}
              >
                <Download size={15} />
                {versionsLoading ? "Загружаем версии…" : "Скачать файл"}
              </button>
              {offerBankAlt && (
                <button
                  type="button"
                  className="btn-secondary w-full"
                  onClick={() => {
                    setOffer(null);
                    setBankAlt(offerBankAlt);
                  }}
                >
                  <Wand2 size={15} />
                  Альтернативный способ · {bankDisplayName(offerBankAlt)}
                </button>
              )}
              {offerActivation && (
                <button
                  className="btn-secondary w-full"
                  onClick={() => setActivation({
                    appTitle: offer.app_title,
                                                                   
                    data: offerActivation,
                  })}
                >
                  <QrCode size={15} />Активация · все QR ({offerActivation.items.length})
                </button>
              )}
              {offer.show_account_versions && (
                <button
                  data-tour="sheet-other"
                  className="btn-secondary w-full"
                  onClick={() => void scanAppRestoreVersions(offer, false)}
                >
                  <History size={15} />Выбрать другую версию
                </button>
              )}
              {exportQueued && (
                <button className="btn-secondary w-full" onClick={() => void queueOpenExportsFolder()}>
                  <FolderOpen size={15} />Открыть папку Signum
                </button>
              )}
              <button className="btn-secondary w-full" onClick={() => { setOffer(null); setResolveError(""); }}>Отмена</button>
            </div>
          </div>
        </div>
      )}

      {activation && (
        <ActivationModal
          appTitle={activation.appTitle}
          activation={activation.data}
          onClose={() => setActivation(null)}
        />
      )}

      {bankAlt && (
        <BankAltInstallModal bank={bankAlt} onClose={() => setBankAlt(null)} />
      )}
    </div>
  );
}

function compareIos(deviceIos: string, minIos: string): boolean {
  const parts = (value: string) => value.split(/\D+/).filter(Boolean).map(v => Number.parseInt(v, 10) || 0);
  const a = parts(deviceIos);
  const b = parts(minIos);
  const len = Math.max(a.length, b.length);
  for (let i = 0; i < len; i += 1) {
    if ((a[i] ?? 0) < (b[i] ?? 0)) return false;
    if ((a[i] ?? 0) > (b[i] ?? 0)) return true;
  }
  return true;
}

                                                                        
function formatAppStoreVersionLabel(version: string, externalVersionId: string): string {
  const v = (version || "").trim();
  const id = (externalVersionId || "").trim();
  if (!v) return id ? `сборка ${id}` : "—";
                                                                                                    
  if (id && v === id) return `сборка ${id}`;
  if (/^\d{8,}$/.test(v)) return `сборка ${v}`;
  return v.startsWith("v") || v.startsWith("V") ? v : `v${v}`;
}

                                                                       
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
