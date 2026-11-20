import {
  CheckCircle2, Download, Eye, EyeOff, Globe, History, KeyRound, LoaderCircle, LogOut,
  Plus, RefreshCw, Search, Smartphone, Trash2, UserRound, X,
} from "lucide-react";
import { FormEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import Checkbox from "../components/Checkbox";
import PageHeader from "../components/PageHeader";
import SelectMenu from "../components/SelectMenu";
import { formatAuthError, isTwoFactorError } from "../lib/auth";
import { cacheBustUrl } from "../lib/media";
import { command, errorMessage, logUi } from "../lib/tauri";
import {
  TOUR_CODE,
  TOUR_EMAIL,
  tourHistoryRows,
  useTourDemo,
  useTourSignal,
} from "../lib/tourStore";
import type {
  AuthStatus,
  ChangeRegionResult,
  GeneratedAddress,
  LoginAppleResult,
  PurchaseHistoryApp,
  PurchaseHistoryResult,
  SessionState,
  StoreRegionCacheEntry,
  StoreRegionOption,
  StoreRegionStatus,
} from "../types";

function accountStateLabel(state: SessionState): string {
  switch (state) {
    case "active":
    case "saved":
      return "Готов к восстановлению";
    default:
      return "Нужно подключить";
  }
}

                                                                           
function normalizeSearchText(value: string): string {
  return (value || "")
    .normalize("NFKD")
    .replace(/\p{M}/gu, "")
    .toLocaleLowerCase();
}

function appMatchesQuery(item: PurchaseHistoryApp, query: string): boolean {
  const q = normalizeSearchText(query).trim();
  if (!q) return true;
  const tokens = q.split(/\s+/).filter(Boolean);
  const hay = normalizeSearchText(
    [item.name, item.artist, item.bundleId, item.adamId].filter(Boolean).join(" "),
  );
  return tokens.every(t => hay.includes(t));
}

type LoginStep = "credentials" | "two_factor" | "history_two_factor";

export default function Account() {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
                                                                             
  const passwordRef = useRef("");
  const [code, setCode] = useState("");
  const [step, setStep] = useState<LoginStep>("credentials");
  const [showPassword, setShowPassword] = useState(false);
  const [showLoginForm, setShowLoginForm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
                                                                           
  const [authReady, setAuthReady] = useState(false);
  const [status, setStatus] = useState<AuthStatus>({ authenticated: false, accounts: [] });
  const [historyPasswordSaved, setHistoryPasswordSaved] = useState(false);
  const [historySessionSaved, setHistorySessionSaved] = useState(false);
  const [history2fa, setHistory2fa] = useState("");
  const [historyNeeds2fa, setHistoryNeeds2fa] = useState(false);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historyLoadingMore, setHistoryLoadingMore] = useState(false);
  const [historyLoadingAll, setHistoryLoadingAll] = useState(false);
  const [historyLoaded, setHistoryLoaded] = useState(false);
  const [historyItems, setHistoryItems] = useState<PurchaseHistoryApp[]>([]);
  const [historyHasMore, setHistoryHasMore] = useState(false);
  const [historyNextBatchId, setHistoryNextBatchId] = useState<string | null>(null);
  const [historySearch, setHistorySearch] = useState("");
  const [historyMessage, setHistoryMessage] = useState("");
  const [historyError, setHistoryError] = useState("");
  const [historyActionId, setHistoryActionId] = useState<string | null>(null);
  const [selectedHistoryIds, setSelectedHistoryIds] = useState<string[]>([]);
  const [historyBatchBusy, setHistoryBatchBusy] = useState<"install" | "export" | null>(null);
                                                                                           
  const historyAccountEmail = useRef<string | null>(null);
  const [iconStamp] = useState(() => Date.now());
  const [regionOptions, setRegionOptions] = useState<StoreRegionOption[]>([]);
  const [regionCurrent, setRegionCurrent] = useState("");
  const [regionCurrentCode, setRegionCurrentCode] = useState("");
  const [regionCachedAt, setRegionCachedAt] = useState<number | null>(null);
  const [regionTarget, setRegionTarget] = useState("");
  const [regionAddress, setRegionAddress] = useState<GeneratedAddress | null>(null);
  const [regionAddresses, setRegionAddresses] = useState<GeneratedAddress[]>([]);
  const [regionBusy, setRegionBusy] = useState(false);
  const [regionMessage, setRegionMessage] = useState("");
  const [regionError, setRegionError] = useState("");
  const [regionNeeds2fa, setRegionNeeds2fa] = useState(false);
  const [region2fa, setRegion2fa] = useState("");
  const [regionReady, setRegionReady] = useState(false);
  const regionIntent = useRef<"read" | "change">("read");
                                                                                       
  const regionAddressRequest = useRef(0);

  const load = useCallback(() =>
    command<AuthStatus>("get_auth_status")
      .then(s => {
        setStatus(s);
        setHistoryPasswordSaved(!!s.historyPasswordSaved);
        setHistorySessionSaved(!!s.historySessionSaved);
      })
      .catch(() => undefined)
      .finally(() => setAuthReady(true)),
  []);
  useEffect(() => { void load(); }, [load]);

  const pickAddress = useCallback(async (country: string) => {
    const requestId = ++regionAddressRequest.current;
    const code = country.trim();
    if (!code) {
      setRegionAddress(null);
      setRegionAddresses([]);
      return;
    }
    try {
      const addresses = await command<GeneratedAddress[]>("list_store_region_addresses", { country: code });
      if (requestId !== regionAddressRequest.current) return;
      setRegionAddresses(addresses);
      setRegionAddress(addresses[0] ?? null);
      setRegionError("");
    } catch (e) {
      if (requestId !== regionAddressRequest.current) return;
      setRegionAddress(null);
      setRegionAddresses([]);
      setRegionError(errorMessage(e));
    }
  }, []);

  async function refreshStoreRegion(twoFactorCode?: string) {
    if (!hasAccountProfile || regionBusy) return;
    regionIntent.current = "read";
    setRegionBusy(true);
    setRegionError("");
    try {
      const result = await command<StoreRegionStatus>("get_store_region", {
        input: {
          email: activeEmail,
          twoFactorCode: twoFactorCode || null,
        },
      });
      if (result.needs2fa) {
        setRegionNeeds2fa(true);
        setRegion2fa("");
        setRegionMessage(result.message || "Введите 6-значный код с iPhone.");
        return;
      }
      setRegionNeeds2fa(false);
      setRegion2fa("");
      setRegionCurrent(result.countryName || "");
      setRegionCurrentCode(result.countryCode || "");
      setRegionCachedAt(Math.floor(Date.now() / 1000));
      setRegionReady(true);
      setRegionMessage(result.message || "");
      if (result.countryCode && regionOptions.some(o => o.code === result.countryCode)) {
        setRegionTarget(result.countryCode);
        void pickAddress(result.countryCode);
      }
    } catch (e) {
      const msg = errorMessage(e);
      const submittedCode = (twoFactorCode || "").replace(/\D/g, "").slice(0, 6);
      const codeRejected = /код не принят|неверн(?:ый|ого).*код|устаревш(?:ий|его).*код|invalid\s*(?:2fa|verification|auth).*code/i.test(msg);
      if (!codeRejected && submittedCode.length >= 6) {
        setRegionNeeds2fa(false);
        setRegion2fa("");
      }
      setRegionError(msg);
    } finally {
      setRegionBusy(false);
    }
  }

  async function changeStoreRegion() {
    if (!hasAccountProfile || regionBusy || !regionTarget) return;
    if (!regionAddress || regionAddress.countryCode.toUpperCase() !== regionTarget.toUpperCase()) {
      setRegionError("Подбираем адрес для выбранной страны. Подождите немного и повторите.");
      return;
    }
    regionIntent.current = "change";
    const digits = region2fa.replace(/\D/g, "").slice(0, 6);
    setRegionBusy(true);
    setRegionError("");
    setRegionMessage("Меняем страну…");
    try {
      const result = await command<ChangeRegionResult>("change_store_region", {
        input: {
          email: activeEmail,
          country: regionTarget,
          address: regionAddress,
          twoFactorCode: digits.length >= 6 ? digits : null,
        },
      });
      if (result.needs2fa) {
        setRegionNeeds2fa(true);
        setRegion2fa("");
        setRegionMessage(result.message || "Введите 6-значный код с iPhone.");
        return;
      }
      setRegionNeeds2fa(false);
      setRegion2fa("");
      if (result.ok) {
        setRegionCurrent(result.countryName);
        setRegionCurrentCode(result.countryCode);
        setRegionCachedAt(Math.floor(Date.now() / 1000));
        setRegionReady(true);
        setRegionMessage(result.message || "Страна магазина обновлена.");
      } else {
        setRegionError(result.message || "Не удалось сменить страну.");
        setRegionMessage("");
      }
    } catch (e) {
      const msg = errorMessage(e);
      const codeRejected = /код не принят|неверн(?:ый|ого).*код|устаревш(?:ий|его).*код|invalid\s*(?:2fa|verification|auth).*code/i.test(msg);
      if (!codeRejected && digits.length >= 6) {
        setRegionNeeds2fa(false);
        setRegion2fa("");
      }
      setRegionError(msg);
      setRegionMessage("");
    } finally {
      setRegionBusy(false);
    }
  }

  async function resendRegionCode() {
    if (!hasAccountProfile || regionBusy) return;
    setRegionBusy(true);
    setRegionError("");
    setRegionMessage("Отправляем новый код…");
    try {
      if (regionIntent.current === "change") {
        const result = await command<ChangeRegionResult>("change_store_region", {
          input: {
            email: activeEmail,
            country: regionTarget,
            address: regionAddress,
            twoFactorCode: null,
            resendCode: true,
          },
        });
        setRegionMessage(result.message || "Новый код отправлен.");
      } else {
        const result = await command<StoreRegionStatus>("get_store_region", {
          input: {
            email: activeEmail,
            twoFactorCode: null,
            resendCode: true,
          },
        });
        setRegionMessage(result.message || "Новый код отправлен.");
      }
      setRegionNeeds2fa(true);
      setRegion2fa("");
    } catch (e) {
      setRegionError(errorMessage(e));
      setRegionMessage("");
    } finally {
      setRegionBusy(false);
    }
  }

                                                                          
                                                                     
  const activeEmail =
    status.email
    || status.accounts.find(a => a.active)?.email
    || status.accounts[0]?.email
    || "";
  const hasAccountProfile = !!activeEmail;
  const hasAddressForRegion = !!regionAddress
    && regionAddress.countryCode.toUpperCase() === regionTarget.toUpperCase();
  const canLoadHistory = status.authenticated || hasAccountProfile;

  useEffect(() => {
                                                                                  
    ++regionAddressRequest.current;
    if (!hasAccountProfile) {
      setRegionCurrent("");
      setRegionCurrentCode("");
      setRegionCachedAt(null);
      setRegionReady(false);
      setRegionMessage("");
      setRegionError("");
      setRegionAddress(null);
      setRegionAddresses([]);
      return;
    }

    let cancelled = false;
    setRegionCurrent("");
    setRegionCurrentCode("");
    setRegionCachedAt(null);
    setRegionReady(false);
    setRegionMessage("");
    setRegionError("");
    setRegionAddress(null);
    setRegionAddresses([]);

    void (async () => {
      const [list, cached] = await Promise.all([
        command<StoreRegionOption[]>("list_store_regions").catch(() => []),
        command<StoreRegionCacheEntry | null>("get_cached_store_region", {
          input: { email: activeEmail },
        }).catch(() => null),
      ]);
      if (cancelled) return;

      setRegionOptions(list);
      const cachedCode = cached?.countryCode || "";
      const selected = list.some(option => option.code === cachedCode)
        ? cachedCode
        : (list[0]?.code || "");
      setRegionTarget(selected);
      if (selected) void pickAddress(selected);

      if (cached?.countryCode) {
        setRegionCurrent(cached.countryName || "");
        setRegionCurrentCode(cached.countryCode);
        setRegionCachedAt(cached.timestamp || null);
        setRegionReady(true);
      }
    })();

    return () => { cancelled = true; };
  }, [activeEmail, hasAccountProfile, pickAddress]);

  const openHistory2faForm = useCallback((opts?: { keepList?: boolean }) => {
                                                             
                                                                                   
    if (!opts?.keepList) {
      setHistoryItems([]);
      setHistoryLoaded(false);
    }
    setHistoryError("");
    setHistoryPasswordSaved(true);
    setHistory2fa("");
    setHistoryNeeds2fa(true);
                                                                          
    setHistoryMessage("Введите 6-значный код с iPhone.");
    setMessage("");
    setStep("credentials");
    requestAnimationFrame(() => {
      document.getElementById("history-2fa-form")?.scrollIntoView({
        behavior: "smooth",
        block: "center",
      });
    });
  }, []);

  const applyHistoryResult = useCallback((result: PurchaseHistoryResult, opts?: { forceRefresh?: boolean }) => {
    const resultEmail = (result.email || "").trim().toLocaleLowerCase();
    const currentEmail = activeEmail.trim().toLocaleLowerCase();
                                                                                  
                                                                     
    if (resultEmail && currentEmail && resultEmail !== currentEmail) return;
    historyAccountEmail.current = resultEmail || currentEmail || null;
    const apps = result.apps || [];
    const nextId = (result.nextBatchId || "").trim() || null;
    const hasMore = !!result.hasMore && !!nextId;
                                                               
    setHistoryNeeds2fa(false);
    setHistory2fa("");
    setHistoryItems(apps);
    setHistoryLoaded(true);
    setHistoryError("");
    setHistoryPasswordSaved(true);
    setHistorySessionSaved(!!result.sessionSaved || historySessionSaved);
    setHistoryHasMore(hasMore);
    setHistoryNextBatchId(hasMore ? nextId : null);
    const backendMsg = (result.message || "").trim();
    if (backendMsg) {
      setHistoryMessage(backendMsg);
      setMessage(backendMsg);
    } else if (result.fromCache) {
                                                                 
      setHistoryMessage("");
      setMessage("");
    } else if (opts?.forceRefresh) {
      setHistoryMessage("История обновлена.");
      setMessage("История обновлена.");
    } else {
      setHistoryMessage("");
      setMessage("");
    }
  }, [activeEmail, historySessionSaved]);

  const loadPurchaseHistory = useCallback(async (
    with2fa?: string,
    opts?: { loadMore?: boolean; loadAll?: boolean; forceRefresh?: boolean },
  ) => {
    const loadMore = !!opts?.loadMore;
    const forceRefresh = !!opts?.forceRefresh;
    if (historyLoading || historyLoadingMore || busy) return;
                                                                                
    if (step === "two_factor") return;
    if (!activeEmail) {
      setHistoryError("Сначала войдите в Apple ID.");
      return;
    }
    if (loadMore && !historyNextBatchId) {
      setHistoryHasMore(false);
      setHistoryError("");
      return;
    }
                                                                                       
                                                                                
    const codeDigits = loadMore
      ? ""
      : (with2fa ?? history2fa).replace(/\D/g, "").slice(0, 6);
    if (historyNeeds2fa && codeDigits.length < 6 && !loadMore) {
      setHistoryError("Введите 6-значный код с iPhone.");
      openHistory2faForm();
      return;
    }
    if (loadMore) {
      setHistoryLoadingMore(true);
    } else {
      setHistoryLoading(true);
    }
    setHistoryError("");
    setHistoryMessage("");
    try {
      const result = await command<PurchaseHistoryResult>("fetch_purchase_history", {
        input: {
          email: activeEmail,
          password: null,
          twoFactorCode: codeDigits.length >= 6 ? codeDigits : null,
          loadMore,
          forceRefresh: loadMore ? false : forceRefresh,
          batchId: loadMore ? historyNextBatchId : null,
          loadAll: !loadMore && opts?.loadAll ? true : null,
        },
      });
      if (result.needs2fa) {
                                                                                        
        openHistory2faForm({
          keepList: historyItems.length > 0 || historyLoaded,
        });
        return;
      }
      applyHistoryResult(result, { forceRefresh: forceRefresh && !loadMore });
      void load();
    } catch (e) {
      const msg = errorMessage(e);
      if (!loadMore && historyItems.length === 0) {
        setHistoryLoaded(false);
      }
                                                                                   
                                                                                  
      const codeRejected = /код не принят|неверн(?:ый|ого).*код|устаревш(?:ий|его).*код|invalid\s*(?:2fa|verification|auth).*code/i.test(msg);
      const keepForm = codeRejected && !loadMore;
      if (keepForm) {
        setHistory2fa("");
        setHistoryNeeds2fa(true);
        setHistoryError("");
        openHistory2faForm({
          keepList: historyItems.length > 0,
        });
      } else {
        if (!loadMore) {
          setHistoryNeeds2fa(false);
          setHistory2fa("");
        }
        setHistoryError(msg);
        setHistoryMessage("");
        setMessage(msg);
      }
      if (/неверный пароль|пароль не найден|войдите/i.test(msg)) {
        setHistoryPasswordSaved(false);
      }
      if (/сессия/i.test(msg)) {
        setHistorySessionSaved(false);
      }
    } finally {
      setHistoryLoading(false);
      setHistoryLoadingMore(false);
    }
  }, [
    historyLoading,
    historyLoadingMore,
    busy,
    step,
    status.authenticated,
    activeEmail,
    history2fa,
    historyNeeds2fa,
    historyNextBatchId,
    historyItems.length,
    openHistory2faForm,
    applyHistoryResult,
  ]);

  const loadAllPurchaseHistory = useCallback(async () => {
    if (historyLoadingAll || historyLoading || historyLoadingMore || busy || !historyHasMore) return;
    setHistoryLoadingAll(true);
    setHistoryError("");
    setHistoryMessage("Загружаем историю до конца…");
    try {
      await loadPurchaseHistory(undefined, { loadAll: true });
    } finally {
      setHistoryLoadingAll(false);
    }
  }, [historyLoadingAll, historyLoading, historyLoadingMore, busy, historyHasMore, loadPurchaseHistory]);

  useEffect(() => {
                                                                               
                                                                                    
    if (!hasAccountProfile) {
      setHistoryItems([]);
      setHistoryLoaded(false);
      setHistoryMessage("");
      setHistoryError("");
      setHistoryPasswordSaved(false);
      setHistorySessionSaved(false);
      setHistoryHasMore(false);
      setHistoryNextBatchId(null);
      setHistorySearch("");
      setHistory2fa("");
      setHistoryNeeds2fa(false);
      return;
    }
    setHistoryPasswordSaved(!!status.historyPasswordSaved);
    setHistorySessionSaved(!!status.historySessionSaved);
  }, [hasAccountProfile, status.historyPasswordSaved, status.historySessionSaved]);

                                                                             
                                                                             
                                                              
  useEffect(() => {
    if (!activeEmail) {
      historyAccountEmail.current = null;
      return;
    }
    const currentEmail = activeEmail.trim().toLocaleLowerCase();
    if (!historyAccountEmail.current) {
      if (historyLoaded) historyAccountEmail.current = currentEmail;
      return;
    }
    if (historyAccountEmail.current === currentEmail) return;

    historyAccountEmail.current = currentEmail;
    setHistoryItems([]);
    setHistoryLoaded(false);
    setHistoryMessage("");
    setHistoryError("");
    setHistoryHasMore(false);
    setHistoryNextBatchId(null);
    setHistorySearch("");
    setHistory2fa("");
    setHistoryNeeds2fa(false);
    setSelectedHistoryIds([]);
  }, [activeEmail, historyLoaded]);

  // While the walkthrough runs, this page shows demo rows: it can explain the
  // history, the country picker and the login form without a real Apple ID.
  const tourDemo = useTourDemo();
  const tourSignal = useTourSignal();
  const [tourEmail, setTourEmail] = useState("");
  const [tourPassword, setTourPassword] = useState("");
  const [tourCode, setTourCode] = useState("");

  useEffect(() => {
    if (!tourDemo) {
      setTourEmail("");
      setTourPassword("");
      setTourCode("");
      return;
    }
    if (tourSignal.name === "account-credentials") {
      setTourEmail(TOUR_EMAIL);
      setTourPassword("••••••••••");
      setTourCode("");
      return;
    }
    if (tourSignal.name === "account-2fa") {
      setTourEmail(TOUR_EMAIL);
      setTourPassword("••••••••••");
      setTourCode(TOUR_CODE);
      return;
    }
    if (tourSignal.name !== "account-typing") return;
    setTourEmail("");
    setTourPassword("");
    setTourCode("");
    const timers = TOUR_EMAIL.split("").map((_, i) =>
      window.setTimeout(() => setTourEmail(TOUR_EMAIL.slice(0, i + 1)), 50 * (i + 1)),
    );
    const base = 50 * TOUR_EMAIL.length + 220;
    for (let i = 0; i < 10; i++) {
      timers.push(window.setTimeout(() => setTourPassword("•".repeat(i + 1)), base + 50 * (i + 1)));
    }
    return () => timers.forEach(window.clearTimeout);
  }, [tourDemo, tourSignal]);

  const tourTwoFactor = tourDemo && tourSignal.name === "account-2fa";
  const shownHistory = tourDemo ? tourHistoryRows : historyItems;
  const shownHistoryLoaded = tourDemo ? true : historyLoaded;

  const purchaseRows = useMemo(
    () => shownHistory.filter(item => appMatchesQuery(item, historySearch)),
    [shownHistory, historySearch],
  );
  const selectedHistorySet = useMemo(() => new Set(selectedHistoryIds), [selectedHistoryIds]);
  const visibleHistoryIds = useMemo(
    () => purchaseRows.map(item => item.adamId).filter(Boolean),
    [purchaseRows],
  );
  const allVisibleSelected =
    visibleHistoryIds.length > 0 && visibleHistoryIds.every(id => selectedHistorySet.has(id));
  const selectedCount = selectedHistoryIds.length;

  function historyQueueInput(item: PurchaseHistoryApp) {
    return {
      title: item.name,
      bundle_id: item.bundleId || "",
      app_store_id: item.adamId,
      in_store: false,
      already_owned: true,
      min_ios: null,
      version: null,
    };
  }

  function toggleHistorySelected(id: string) {
    if (!id) return;
    setSelectedHistoryIds(current => (
      current.includes(id) ? current.filter(item => item !== id) : [...current, id]
    ));
  }

  function toggleSelectAllVisible() {
    setSelectedHistoryIds(current => {
      if (allVisibleSelected) {
        return current.filter(id => !visibleHistoryIds.includes(id));
      }
      const next = new Set(current);
      visibleHistoryIds.forEach(id => next.add(id));
      return [...next];
    });
  }

  async function enqueueHistoryItem(item: PurchaseHistoryApp, install: boolean) {
    await command<string>(install ? "enqueue_catalog_install" : "enqueue_catalog_export", {
      input: historyQueueInput(item),
    });
  }

  async function queueInstallFromHistory(item: PurchaseHistoryApp) {
    const key = `${item.adamId}:install`;
    setHistoryActionId(key);
    setHistoryError("");
    try {
      await enqueueHistoryItem(item, true);
      setHistoryMessage(`«${item.name}» в очереди установки. Список можно не обновлять.`);
    } catch (e) {
      setHistoryError(errorMessage(e));
    } finally {
      setHistoryActionId(null);
    }
  }

  async function queueExportFromHistory(item: PurchaseHistoryApp) {
    const key = `${item.adamId}:export`;
    setHistoryActionId(key);
    setHistoryError("");
    try {
      await enqueueHistoryItem(item, false);
      setHistoryMessage(`«${item.name}» в очереди загрузки. Список можно не обновлять.`);
    } catch (e) {
      setHistoryError(errorMessage(e));
    } finally {
      setHistoryActionId(null);
    }
  }

  async function queueSelectedFromHistory(install: boolean) {
    const items = historyItems.filter(item => selectedHistorySet.has(item.adamId));
    if (!items.length) return;
    setHistoryBatchBusy(install ? "install" : "export");
    setHistoryError("");
    let ok = 0;
    const failed: string[] = [];
    for (const item of items) {
      try {
        await enqueueHistoryItem(item, install);
        ok += 1;
      } catch (e) {
        failed.push(item.name);
        setHistoryError(errorMessage(e));
      }
    }
    setHistoryBatchBusy(null);
    if (ok) {
      setSelectedHistoryIds([]);
      setHistoryMessage(
        install
          ? `${ok} в очереди установки. Список можно не обновлять.`
          : `${ok} в очереди загрузки. Список можно не обновлять.`,
      );
    }
    if (failed.length) {
      setHistoryError(
        failed.length === 1
          ? `Не удалось добавить «${failed[0]}».`
          : `Не удалось добавить ${failed.length} приложений.`,
      );
    }
  }

  function resetLogin() {
    setStep("credentials");
    setCode("");
    setPassword("");
    passwordRef.current = "";
    setMessage("");
    setHistoryNeeds2fa(false);
    setHistory2fa("");
  }

  async function applyLoginResult(result: LoginAppleResult) {
    if (result.needs2fa) {
      setEmail(result.email);
      setStep("two_factor");
      setCode("");
      setHistoryNeeds2fa(false);
      setHistory2fa("");
                                                           
      if (password.trim()) passwordRef.current = password;
      setMessage(result.message || "Введите 6-значный код с iPhone.");
      return;
    }

    await load();
    setHistoryPasswordSaved(true);

                                                                                 
                                                                             
    if (result.historyNeeds2fa) {
      setHistoryNeeds2fa(false);
      setHistory2fa("");
    }

                                                    
    setPassword("");
    setCode("");
    setStep("credentials");
    setEmail("");
    setHistoryNeeds2fa(false);
    setHistory2fa("");
    setShowLoginForm(false);

    setMessage(result.message || "Apple ID добавлен.");
  }

  async function login(twoFactorCode?: string) {
    logUi(twoFactorCode ? "ввёл код подтверждения" : "нажал «Войти в Apple ID»");
    if (busy) return;
    const pw = (password || passwordRef.current).trim();
    if (password.trim()) passwordRef.current = password.trim();
    const codeDigits = (twoFactorCode || "").replace(/\D/g, "").slice(0, 6);
    setBusy(true);
    setMessage(
      codeDigits
        ? "Проверяем код…"
        : "Подключаем Apple ID…",
    );
    try {
      const result = await command<LoginAppleResult>("login_apple", {
        input: {
          email,
          password: pw,
                                                                                   
          two_factor_code: codeDigits.length >= 6 ? codeDigits : null,
        },
      });
      await applyLoginResult(result);
    } catch (err) {
      const raw =
        err instanceof Error && err.message
          ? err.message
          : errorMessage(err);
                                                                
      if (!codeDigits && isTwoFactorError(raw)) {
        if (pw) passwordRef.current = pw;
        setStep("two_factor");
        setCode("");
        setMessage("Введите 6-значный код с iPhone.");
        return;
      }
      const shown = formatAuthError(errorMessage(err));
      setMessage(shown);
      if (codeDigits) {
        setStep("two_factor");
        if (/не подошёл|не подошел|устарел/.test(shown.toLowerCase())) {
          setCode("");
        }
      }
    } finally {
      setBusy(false);
    }
  }

  async function submitCredentials(e: FormEvent) {
    e.preventDefault();
    if (busy) return;
    if (password.trim()) passwordRef.current = password.trim();
    await login();
  }

  async function submitTwoFactor(e: FormEvent) {
    e.preventDefault();
    if (busy) return;
    const digits = code.replace(/\D/g, "").slice(0, 6);
    if (digits.length < 6) {
      setMessage("Введите 6-значный код подтверждения.");
      return;
    }
    await login(digits);
  }

  async function submitHistoryTwoFactor(e: FormEvent) {
    e.preventDefault();
                                                     
    if (busy || historyLoading) return;
    const digits = history2fa.replace(/\D/g, "").slice(0, 6);
    if (digits.length < 6) {
      setMessage("Введите 6-значный код с iPhone.");
      openHistory2faForm({
        keepList: historyItems.length > 0,
      });
      return;
    }
    await loadPurchaseHistory(digits);
  }

  async function requestNewCode() {
    logUi("нажал «Запросить новый код»");
    if (busy) return;
    const pw = (password || passwordRef.current).trim();
    setBusy(true);
    setMessage("Запрашиваем новый код…");
    setCode("");
    try {
      const result = await command<LoginAppleResult>("login_apple", {
        input: {
          email,
          password: pw,
          two_factor_code: null,
          resend_code: true,
        },
      });
      setMessage(
        result.message
          || "Новый код отправлен. Смотрите push на iPhone.",
      );
      setStep("two_factor");
    } catch (err) {
      const raw = errorMessage(err);
      if (isTwoFactorError(raw)) {
        setMessage("Новый код отправлен. Смотрите push на iPhone.");
        setStep("two_factor");
      } else {
        setMessage(formatAuthError(raw) || "Не удалось запросить новый код. Подождите минуту и попробуйте снова.");
      }
    } finally {
      setBusy(false);
    }
  }

                                                                                           
  async function requestHistoryCodeResend() {
    if (busy || historyLoading) return;
    setBusy(true);
    setHistoryLoading(true);
    setMessage("Отправляем код повторно…");
    setHistoryMessage("Отправляем код повторно…");
    setHistory2fa("");
    try {
      const result = await command<PurchaseHistoryResult>("fetch_purchase_history", {
        input: {
          email: email || activeEmail,
          password: password.trim() || null,
          twoFactorCode: null,
          resendCode: true,
        },
      });
      if (!result.needs2fa && (result.apps?.length || 0) > 0) {
        setHistoryNeeds2fa(false);
        setHistoryItems(result.apps || []);
        setHistoryLoaded(true);
        setStep("credentials");
        setMessage(result.message || "История покупок готова.");
        setHistoryMessage(result.message || "История покупок готова.");
        await load();
        return;
      }
      openHistory2faForm();
    } catch (e) {
      openHistory2faForm();
      setHistoryError(errorMessage(e));
    } finally {
      setBusy(false);
      setHistoryLoading(false);
    }
  }

  async function retryCatalogLogin() {
    if (busy || !activeEmail) return;
    setBusy(true);
    setMessage("Пробуем подключить восстановление приложений…");
    try {
      const result = await command<LoginAppleResult>("login_apple", {
        input: { email: activeEmail, password: "", two_factor_code: null },
      });
      await applyLoginResult(result);
    } catch (e) {
      setMessage(formatAuthError(errorMessage(e)) || "Сейчас не получается восстановить приложения из каталога. Попробуйте позже.");
    } finally {
      setBusy(false);
    }
  }

  function startRelogin(targetEmail: string) {
    resetLogin();
    setEmail(targetEmail);
    setShowLoginForm(true);
    setMessage("Apple ID уже добавлен. Введите новый пароль, только если вы его изменили.");
    window.requestAnimationFrame(() => {
      document.getElementById("account-login-form")?.scrollIntoView({
        behavior: document.documentElement.classList.contains("lite-simple-scrolling") ? "auto" : "smooth",
        block: "start",
      });
    });
  }

  async function switchAccount(targetEmail: string) {
    setBusy(true);
    setMessage("");
    try {
      await command<void>("switch_apple_account", { input: { email: targetEmail } });
      setMessage(`Активный аккаунт: ${targetEmail}`);
      await load();
    } catch (e) {
      const raw = formatAuthError(errorMessage(e));
      setMessage(raw);
      if (raw.includes("Войдите") || raw.includes("2FA") || raw.includes("устарела")) {
        startRelogin(targetEmail);
      }
    } finally {
      setBusy(false);
    }
  }

  async function removeAccount(targetEmail: string) {
    logUi("нажал «Выйти из Apple ID»");
    setBusy(true);
    setMessage("");
    try {
      await command<void>("remove_apple_account", { input: { email: targetEmail } });
      if (step === "two_factor" && email === targetEmail) resetLogin();
      setMessage("Аккаунт удалён");
      await load();
    } catch (e) {
      setMessage(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  const hasAccounts = status.accounts.length > 0;
  const otherAccounts = status.accounts.filter(a => a.email !== activeEmail);
  const activeAccount =
    status.accounts.find(a => a.active)
    || status.accounts.find(a => a.email === activeEmail)
    || null;
  const historySubtitle = activeEmail
    ? `Покупки · ${activeEmail}`
    : "Войдите в Apple ID, чтобы открыть историю";
  const sessionOk = status.authenticated
    || activeAccount?.session_state === "active"
    || activeAccount?.session_state === "saved";
  const catalogErrorText = (activeAccount?.session_error || status.catalogError || "").trim();
  const catalogUnavailable = !!activeAccount && !sessionOk && (historyPasswordSaved || !!catalogErrorText);
  const canReuseSavedPassword = !!activeEmail && email.trim() === activeEmail && historyPasswordSaved;

  const loginForm = (
    <>
      {(step === "credentials" || (tourDemo && !tourTwoFactor)) && (
        <form id="account-login-form" data-tour="account-credentials" className="mt-5 space-y-4" onSubmit={submitCredentials}>
          <label className="block">
            <span className="mb-2 block text-xs font-medium text-slate-400">Email</span>
            <input
              className="field"
              type="email"
              autoComplete="username"
              value={tourDemo ? tourEmail : email}
              readOnly={tourDemo}
              onChange={e => setEmail(e.target.value)}
              placeholder="name@icloud.com"
              required={!tourDemo}
            />
          </label>
          <label className="block">
            <span className="mb-2 block text-xs font-medium text-slate-400">Пароль</span>
            <div className="relative">
              <input
                className="field pr-12"
                type={showPassword ? "text" : "password"}
                autoComplete="current-password"
                value={tourDemo ? tourPassword : password}
                readOnly={tourDemo}
                onChange={e => setPassword(e.target.value)}
                required={!canReuseSavedPassword && !tourDemo}
              />
              <button
                type="button"
                onClick={() => setShowPassword(!showPassword)}
                className="absolute right-3 top-1/2 -translate-y-1/2 p-2 text-slate-600 hover:text-white"
                aria-label={showPassword ? "Скрыть пароль" : "Показать пароль"}
              >
                {showPassword ? <EyeOff size={17} /> : <Eye size={17} />}
              </button>
            </div>
          </label>
          {canReuseSavedPassword && !password && (
            <p className="text-xs leading-5 text-slate-500">
              Пароль уже сохранён. Оставьте поле пустым, чтобы повторить вход.
            </p>
          )}
          <button type="submit" className="btn-primary w-full" disabled={busy}>
            {busy ? <LoaderCircle className="animate-spin" size={16} /> : <KeyRound size={16} />}
            {busy ? "Подключаем…" : hasAccounts ? "Добавить аккаунт" : "Войти в Apple ID"}
          </button>
        </form>
      )}

      {(step === "two_factor" || tourTwoFactor) && (
        <form data-tour="account-2fa" className="mt-5 space-y-4" onSubmit={submitTwoFactor}>
          <div className="rounded-xl border border-amber-400/20 bg-amber-400/10 p-4 text-xs leading-5 text-slate-400">
            Код для входа в <span className="text-slate-200">{tourDemo ? TOUR_EMAIL : email}</span>. Проверьте уведомления на доверенных устройствах Apple и SMS на доверенном номере.
          </div>
          <label className="block">
            <span className="mb-2 block text-xs font-medium text-slate-400">Код</span>
            <input
              className="field text-center text-lg tracking-[0.35em]"
              inputMode="numeric"
              autoFocus={!tourDemo}
              readOnly={tourDemo}
              value={tourDemo ? tourCode : code}
              onChange={e => setCode(e.target.value.replace(/\D/g, "").slice(0, 6))}
              placeholder="000000"
              required={!tourDemo}
            />
          </label>
          <button
            type="button"
            className="btn-secondary w-full py-1.5 text-sm"
            disabled={busy}
            onClick={() => void requestNewCode()}
          >
            Запросить новый код
          </button>
          <div className="flex gap-2">
            <button type="button" className="btn-secondary flex-1" disabled={busy} onClick={resetLogin}>
              <X size={15} />Назад
            </button>
            <button className="btn-primary flex-[1.4]" disabled={busy || code.length < 6}>
              {busy ? <LoaderCircle className="animate-spin" size={16} /> : <KeyRound size={16} />}
              Подтвердить
            </button>
          </div>
        </form>
      )}
    </>
  );

  if (!authReady) {
    return (
      <div className="mx-auto grid max-w-3xl place-items-center p-7 lg:p-9" style={{ minHeight: "60vh" }}>
        <div className="flex flex-col items-center gap-3 text-slate-500">
          <LoaderCircle size={22} className="animate-spin text-brand-400" />
          <span className="text-xs">Загружаем аккаунт…</span>
        </div>
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-3xl p-7 lg:p-9">
      <PageHeader
        title="Аккаунт"
        subtitle={
          hasAccountProfile
            ? `Сейчас активен ${activeEmail}`
            : "Apple ID, история покупок и добавление аккаунтов"
        }
      />

      {message && (
        <p className={`mt-5 whitespace-pre-wrap rounded-xl border p-3 text-xs leading-5 ${
          message.includes("сейчас не получается восстановить")
            || message.includes("каталога недоступно")
            || message.includes("Технически:")
            ? "border-amber-400/20 bg-amber-400/[0.05] text-amber-100"
            : message.includes("добавлен")
            || message.includes("Активный аккаунт")
            || message.includes("удалён")
            || message.includes("очеред")
            || message.includes("обновлена")
            ? "border-emerald-400/15 text-emerald-300"
            : message.includes("код") || message.includes("доверенного")
              ? "border-amber-400/15 text-amber-200"
              : "border-rose-400/15 text-rose-300"
        }`} role="status">
          {message}
        </p>
      )}

      {                                                               }
      {hasAccountProfile ? (
        <section className="panel mt-6 overflow-hidden p-0">
          <div className="border-b border-white/[0.06] bg-gradient-to-br from-emerald-500/[0.07] via-transparent to-transparent px-6 py-5">
            <div className="flex flex-wrap items-start justify-between gap-4">
              <div className="flex min-w-0 items-start gap-4">
                <div className="grid h-12 w-12 shrink-0 place-items-center rounded-2xl bg-emerald-400/15 text-emerald-300 ring-1 ring-emerald-400/20">
                  <UserRound size={22} />
                </div>
                <div className="min-w-0">
                  <div className="text-[11px] font-medium uppercase tracking-wider text-emerald-300/90">
                    Активный Apple ID
                  </div>
                  <div className="mt-1 truncate text-base font-semibold text-slate-50 sm:text-lg">
                    {activeEmail}
                  </div>
                  <div className="mt-2 flex flex-wrap gap-1.5">
                    {sessionOk ? (
                      <span className="inline-flex items-center gap-1 rounded-md border border-emerald-400/20 bg-emerald-400/10 px-2 py-0.5 text-[10px] font-semibold text-emerald-300">
                        <CheckCircle2 size={11} />
                        Восстановление доступно
                      </span>
                    ) : (
                      <span className="inline-flex items-center gap-1 rounded-md border border-amber-400/25 bg-amber-400/10 px-2 py-0.5 text-[10px] font-semibold text-amber-200">
                        Восстановление недоступно
                      </span>
                    )}
                    {historySessionSaved && (
                      <span className="rounded-md border border-sky-400/15 bg-sky-400/10 px-2 py-0.5 text-[10px] text-sky-300">
                        История без кода
                      </span>
                    )}
                    {historyPasswordSaved && !historySessionSaved && (
                      <span className="rounded-md border border-white/[0.08] bg-white/[0.04] px-2 py-0.5 text-[10px] text-slate-500">
                        Пароль сохранён
                      </span>
                    )}
                  </div>
                </div>
              </div>
              <button
                type="button"
                disabled={busy}
                onClick={() => void removeAccount(activeEmail)}
                className="btn-secondary shrink-0 px-3 py-2 text-[11px] text-rose-300 hover:text-rose-200"
              >
                <LogOut size={14} />
                Выйти
              </button>
            </div>
          </div>

          {catalogUnavailable && (
            <div className="mx-6 mb-5 rounded-xl border border-amber-400/20 bg-amber-400/[0.06] p-4" role="status">
              <div className="text-sm font-semibold text-amber-100">Восстановление приложений недоступно</div>
              <p className="mt-1 text-xs leading-5 text-amber-100/80">
                {catalogErrorText
                  || "Apple ID добавлен. Войдите снова, чтобы загружать приложения из каталога."}
              </p>
              <div className="mt-3 flex flex-wrap gap-2">
                <button type="button" className="btn-primary px-3 py-2 text-xs" disabled={busy} onClick={() => void retryCatalogLogin()}>
                  {busy ? <LoaderCircle className="animate-spin" size={15} /> : <RefreshCw size={15} />}
                  Попробовать снова
                </button>
                <button type="button" className="btn-secondary px-3 py-2 text-xs" disabled={busy} onClick={() => startRelogin(activeEmail)}>
                  Войти снова
                </button>
              </div>
            </div>
          )}

          {!catalogUnavailable && !sessionOk && (
            <div className="mx-6 mb-5 flex flex-wrap gap-2">
              <button type="button" className="btn-secondary px-3 py-2 text-xs" disabled={busy} onClick={() => startRelogin(activeEmail)}>
                Войти снова
              </button>
            </div>
          )}

          {otherAccounts.length > 0 && (
            <div className="px-6 py-4">
              <div className="mb-2 text-[11px] font-medium uppercase tracking-wider text-slate-600">
                Другие аккаунты
              </div>
              <div className="space-y-2">
                {otherAccounts.map(account => (
                  <div
                    key={account.email}
                    className="flex items-center justify-between gap-3 rounded-xl border border-white/[0.06] bg-black/15 px-3.5 py-3"
                  >
                    <div className="min-w-0">
                      <div className="truncate text-sm font-medium text-slate-200">{account.email}</div>
                      <div className="mt-0.5 text-[11px] text-slate-600">
                        {accountStateLabel(account.session_state)}
                      </div>
                    </div>
                    <div className="flex shrink-0 items-center gap-1.5">
                      {(account.session_state === "saved" || account.session_state === "active") ? (
                        <>
                          <button
                            type="button"
                            disabled={busy}
                            onClick={() => void switchAccount(account.email)}
                            className="btn-secondary px-2.5 py-1.5 text-[11px]"
                          >
                            Сделать активным
                          </button>
                          <button
                            type="button"
                            disabled={busy}
                            onClick={() => startRelogin(account.email)}
                            className="btn-secondary px-2.5 py-1.5 text-[11px]"
                          >
                            Войти снова
                          </button>
                        </>
                      ) : (
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => startRelogin(account.email)}
                          className="btn-secondary px-2.5 py-1.5 text-[11px]"
                        >
                          Войти снова
                        </button>
                      )}
                      <button
                        type="button"
                        disabled={busy}
                        onClick={() => void removeAccount(account.email)}
                        className="btn-secondary px-2 py-1.5 text-[11px] text-rose-300/90 hover:text-rose-200"
                        aria-label={`Удалить ${account.email}`}
                      >
                        <Trash2 size={14} />
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}
        </section>
      ) : null}

      {(!hasAccountProfile || showLoginForm || tourDemo) && (
        <section className="panel mt-6 p-6">
          <div className="flex items-center gap-3">
            <div className="rounded-xl bg-brand-500/15 p-3 text-brand-300">
              <UserRound size={20} />
            </div>
            <div>
              <h2 className="font-bold">{hasAccountProfile ? "Войти снова" : "Войти в Apple ID"}</h2>
              <p className="mt-0.5 text-xs text-slate-500">
                {hasAccountProfile ? "Используйте сохранённый пароль или введите новый." : "Нужен для восстановления приложений и истории покупок"}
              </p>
            </div>
          </div>
          {loginForm}
        </section>
      )}

      {                                }
      {(hasAccountProfile || tourDemo) && (
        <section data-tour="account-country" className="panel mt-5 overflow-hidden p-0">
          <div className="border-b border-violet-400/10 bg-gradient-to-br from-violet-500/[0.14] via-violet-500/[0.04] to-transparent px-6 py-5">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div className="flex min-w-0 items-center gap-3">
                <div className="grid h-11 w-11 shrink-0 place-items-center rounded-2xl bg-violet-400/15 text-violet-200 ring-1 ring-violet-400/25">
                  <Globe size={21} />
                </div>
                <div className="min-w-0">
                  <div className="text-[10px] font-semibold uppercase tracking-[0.16em] text-violet-300/80">
                    App Store
                  </div>
                  <h2 className="mt-0.5 font-bold text-slate-100">Страна магазина</h2>
                  <p className="mt-0.5 text-xs text-slate-400">
                    Определяет доступные приложения и варианты адреса
                  </p>
                </div>
              </div>
              <button
                type="button"
                className="btn-secondary shrink-0 border-violet-300/15 bg-violet-400/[0.06] px-3 py-2 text-xs text-violet-100 hover:bg-violet-400/[0.12]"
                disabled={regionBusy || busy}
                onClick={() => void refreshStoreRegion()}
              >
                {regionBusy && !regionNeeds2fa ? (
                  <LoaderCircle size={14} className="animate-spin" />
                ) : (
                  <RefreshCw size={14} />
                )}
                {regionReady ? "Проверить" : "Узнать"}
              </button>
            </div>
          </div>

          <div className="space-y-4 px-6 py-5">

          {regionReady && regionCurrent ? (
            <div className="rounded-2xl border border-violet-400/15 bg-violet-400/[0.06] p-4 ring-1 ring-violet-400/[0.05]">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                  <div className="text-[10px] font-semibold uppercase tracking-[0.14em] text-violet-200/65">
                    Текущая страна
                  </div>
                  <div className="mt-1 text-lg font-semibold text-slate-50">{regionCurrent}</div>
                  {regionCachedAt && (
                    <p className="mt-1.5 text-[11px] text-slate-500">
                      Проверено {new Date(regionCachedAt * 1000).toLocaleString("ru-RU")}
                    </p>
                  )}
                </div>
                {regionCurrentCode && (
                  <span className="rounded-lg border border-violet-300/20 bg-violet-400/10 px-2.5 py-1 text-[11px] font-semibold tracking-wider text-violet-100">
                    {regionCurrentCode}
                  </span>
                )}
              </div>
            </div>
          ) : (
            <div className="rounded-2xl border border-white/[0.07] bg-black/10 px-4 py-3 text-xs text-slate-500">
              Нажмите «Узнать», чтобы проверить текущую страну магазина.
            </div>
          )}

          {regionMessage && !regionError && (
            <p className="mt-4 rounded-xl border border-emerald-400/15 bg-emerald-400/5 px-4 py-2.5 text-xs text-emerald-300/90">
              {regionMessage}
            </p>
          )}
          {regionError && (
            <p className="mt-4 rounded-xl border border-rose-400/15 bg-rose-400/5 px-4 py-2.5 text-xs text-rose-300">
              {regionError}
            </p>
          )}

          {regionNeeds2fa && (
            <div className="mt-4 rounded-xl border border-amber-400/30 bg-amber-400/10 p-4">
              <div className="text-sm font-semibold text-amber-100">Код с iPhone</div>
              <p className="mt-1 text-[11px] text-amber-100/70">
                Apple просит подтвердить вход
              </p>
              <input
                className="field mt-3 text-center text-lg tracking-[0.35em]"
                inputMode="numeric"
                autoComplete="one-time-code"
                value={region2fa}
                onChange={e => setRegion2fa(e.target.value.replace(/\D/g, "").slice(0, 6))}
                placeholder="000000"
                disabled={regionBusy}
              />
              <div className="mt-3 flex flex-wrap gap-2">
                <button
                  type="button"
                  className="btn-primary flex-1 text-xs"
                  disabled={regionBusy || region2fa.length < 6}
                  onClick={() => {
                    if (regionIntent.current === "change") void changeStoreRegion();
                    else void refreshStoreRegion(region2fa);
                  }}
                >
                  {regionBusy ? <LoaderCircle size={14} className="animate-spin" /> : <KeyRound size={14} />}
                  Подтвердить
                </button>
                <button
                  type="button"
                  className="btn-secondary flex-1 text-xs"
                  disabled={regionBusy}
                  onClick={() => void resendRegionCode()}
                >
                  <RefreshCw size={14} />
                  Отправить код ещё раз
                </button>
              </div>
            </div>
          )}

          <div className="rounded-2xl border border-white/[0.07] bg-black/10 p-4">
            <div className="mb-4 flex items-start gap-3">
              <div className="grid h-7 w-7 shrink-0 place-items-center rounded-lg bg-white/[0.06] text-xs font-semibold text-slate-300">1</div>
              <div>
                <div className="text-sm font-semibold text-slate-200">Выберите новую страну</div>
                <p className="mt-0.5 text-[11px] leading-4 text-slate-500">Мы подберём готовый адрес для выбранного магазина.</p>
              </div>
            </div>

          <label className="block">
            <span className="mb-2 block text-[11px] font-medium uppercase tracking-wider text-slate-500">Страна</span>
            <SelectMenu
              value={regionTarget}
              disabled={regionBusy}
              placeholder="Выберите страну"
              searchPlaceholder="Найти страну…"
              options={regionOptions.map(opt => ({
                value: opt.code,
                label: opt.name,
                hint: opt.code === regionCurrentCode ? "сейчас" : undefined,
              }))}
              onChange={next => {
                setRegionTarget(next);
                void pickAddress(next);
              }}
            />
          </label>

          {regionTarget && !hasAddressForRegion && !regionError && (
            <p className="mt-4 flex items-center gap-2 text-xs text-slate-500">
              <LoaderCircle size={14} className="animate-spin text-brand-400" />
              Подбираем адрес для выбранной страны…
            </p>
          )}

          {regionAddress && (
            <div className="mt-4 rounded-xl border border-white/[0.06] bg-black/15 px-4 py-3 text-xs text-slate-400">
              <div className="mb-1 text-[11px] font-medium uppercase tracking-wider text-slate-600">
                Адрес для этой страны
              </div>
              <div className="text-slate-200">
                {regionAddress.street}
              </div>
              <div>
                {regionAddress.city}
                {regionAddress.state ? `, ${regionAddress.state}` : ""}
                {regionAddress.county ? `, ${regionAddress.county}` : ""}
                {regionAddress.postalCode ? ` · ${regionAddress.postalCode}` : ""}
              </div>
              {regionAddresses.length > 1 && (
                <SelectMenu
                  className="mt-3"
                  disabled={regionBusy}
                  placeholder="Выберите адрес"
                  searchPlaceholder="Найти адрес…"
                  value={String(Math.max(0, regionAddresses.findIndex(address =>
                    address.street === regionAddress.street
                    && address.postalCode === regionAddress.postalCode
                  )))}
                  options={regionAddresses.map((address, index) => ({
                    value: String(index),
                    label: `${address.city}${address.state ? `, ${address.state}` : ""} — ${address.street}`,
                    hint: address.postalCode || undefined,
                  }))}
                  onChange={next => setRegionAddress(regionAddresses[Number(next)] ?? regionAddresses[0])}
                />
              )}
            </div>
          )}

          <button
            type="button"
            className="btn-primary mt-5 w-full shadow-lg shadow-brand-500/10"
            disabled={
              regionBusy
              || busy
              || !regionTarget
              || !hasAddressForRegion
              || (!!regionCurrentCode && regionTarget === regionCurrentCode)
            }
            onClick={() => void changeStoreRegion()}
          >
            {regionBusy || !hasAddressForRegion ? <LoaderCircle size={16} className="animate-spin" /> : <Globe size={16} />}
            {regionBusy ? "Меняем страну…" : !hasAddressForRegion ? "Подбираем адрес…" : "Сменить страну"}
          </button>
          </div>
          </div>
        </section>
      )}

      {                                                        }
      <section data-tour="account-history" className="panel mt-5 p-6">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex min-w-0 items-center gap-3">
            <div className="rounded-xl bg-sky-500/15 p-3 text-sky-300">
              <History size={20} />
            </div>
            <div className="min-w-0">
              <h2 className="font-bold">История покупок</h2>
              <p className="mt-0.5 truncate text-xs text-slate-500">{historySubtitle}</p>
            </div>
          </div>
          {hasAccountProfile && !historyNeeds2fa && (
            <div className="flex flex-wrap justify-end gap-2">
              <button
                type="button"
                className="btn-secondary px-3 py-2.5 text-xs"
                disabled={historyLoading || historyLoadingMore || historyLoadingAll || busy || !canLoadHistory || !historyHasMore}
                onClick={() => void loadAllPurchaseHistory()}
                title="Загрузить все страницы истории"
              >
                {historyLoadingAll ? <LoaderCircle size={14} className="animate-spin" /> : <Download size={14} />}
                {historyLoadingAll ? "Загрузка…" : "До конца"}
              </button>
              <button
                type="button"
                className="btn-primary px-4 py-2.5 text-xs"
                disabled={historyLoading || historyLoadingMore || historyLoadingAll || busy || !canLoadHistory}
                onClick={() => void loadPurchaseHistory(undefined, { forceRefresh: true })}
              >
                {historyLoading ? <LoaderCircle size={14} className="animate-spin" /> : <RefreshCw size={14} />}
                {historyLoading ? "…" : historyLoaded ? "Обновить" : "Загрузить"}
              </button>
            </div>
          )}
        </div>

        {!hasAccountProfile ? (
          <p className="mt-5 rounded-xl border border-white/[0.06] bg-black/10 px-4 py-3 text-xs text-slate-500">
            Сначала войдите в Apple ID выше — история привязана к активному аккаунту.
          </p>
        ) : (
          <>
            {!historyNeeds2fa && shownHistoryLoaded && shownHistory.length > 0 && (
              <label className="relative mt-5 block">
                <Search
                  size={14}
                  className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-slate-500"
                />
                <input
                  className="field w-full py-2.5 pl-9 pr-9 text-xs"
                  type="search"
                  value={historySearch}
                  onChange={e => setHistorySearch(e.target.value)}
                  placeholder="Поиск"
                  disabled={historyLoading}
                />
                {historySearch && (
                  <button
                    type="button"
                    className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-slate-500 hover:text-white"
                    onClick={() => setHistorySearch("")}
                    aria-label="Очистить"
                  >
                    <X size={14} />
                  </button>
                )}
              </label>
            )}

            {historyMessage && !historyError && !historyNeeds2fa && (
              <p className="mt-4 rounded-xl border border-emerald-400/15 bg-emerald-400/5 px-4 py-2.5 text-xs text-emerald-300/90">
                {historyMessage}
              </p>
            )}

            {                                                                  
                                                                     
                                                                            
                                             }
            {historyNeeds2fa && (
              <div
                id="history-2fa-form"
                className="mt-5 rounded-xl border border-amber-400/30 bg-amber-400/10 p-4 ring-1 ring-amber-400/20"
              >
                <div className="text-sm font-semibold text-amber-100">Код для истории</div>
                <p className="mt-1 text-[11px] text-amber-100/70">
                  Для {activeEmail || "текущего Apple ID"} · 6 цифр с iPhone
                </p>
                {(busy || historyLoading) && (
                  <div className="mt-2 flex items-center gap-2 text-xs text-slate-400">
                    <LoaderCircle size={14} className="animate-spin text-brand-400" />
                    …
                  </div>
                )}
                <label className="mt-3 block">
                  <input
                    className="field text-center text-lg tracking-[0.35em]"
                    inputMode="numeric"
                    autoComplete="one-time-code"
                    autoFocus
                    value={history2fa}
                    onChange={e => setHistory2fa(e.target.value.replace(/\D/g, "").slice(0, 6))}
                    onKeyDown={e => {
                      if (e.key !== "Enter") return;
                      e.preventDefault();
                      if (historyLoading || busy) return;
                      const codeDigits = history2fa.replace(/\D/g, "").slice(0, 6);
                      if (codeDigits.length < 6) return;
                      void loadPurchaseHistory(codeDigits);
                    }}
                    placeholder="000000"
                    disabled={historyLoading || busy}
                  />
                </label>
                <div className="mt-3 flex flex-col gap-2 sm:flex-row sm:flex-wrap">
                  <button
                    type="button"
                    className="btn-primary flex-1 text-xs"
                    disabled={historyLoading || busy || history2fa.length < 6}
                    onClick={() => void loadPurchaseHistory(history2fa)}
                  >
                    {historyLoading || busy ? (
                      <LoaderCircle size={14} className="animate-spin" />
                    ) : (
                      <KeyRound size={14} />
                    )}
                    Подтвердить
                  </button>
                  <button
                    type="button"
                    className="btn-secondary text-xs"
                    disabled={historyLoading || busy}
                    onClick={() => void requestHistoryCodeResend()}
                  >
                    <RefreshCw size={14} />
                    Ещё раз
                  </button>
                  <button
                    type="button"
                    className="btn-secondary text-xs"
                    disabled={historyLoading || busy}
                    onClick={() => {
                      setHistoryNeeds2fa(false);
                      setHistory2fa("");
                      setHistoryMessage("");
                      setHistoryError("");
                      setMessage("");
                    }}
                  >
                    Позже
                  </button>
                </div>
              </div>
            )}
          </>
        )}

        {historyLoading && !historyLoaded && (
          <div className="mt-8 grid min-h-28 place-items-center">
            <LoaderCircle size={18} className="animate-spin text-brand-400" />
          </div>
        )}

        {!historyLoading && !historyLoadingMore && historyError && (
          <p className="mt-5 rounded-xl border border-rose-400/15 bg-rose-400/5 px-4 py-3 text-xs text-rose-300">
            {historyError}
          </p>
        )}

        {!historyNeeds2fa && shownHistoryLoaded && shownHistory.length === 0 && !historyError && hasAccountProfile && (
          <p className="mt-5 rounded-xl border border-white/[0.06] bg-black/10 px-4 py-3 text-xs text-slate-500">
            Покупок не найдено для {activeEmail}
          </p>
        )}

        {!historyNeeds2fa && historyLoaded && historyItems.length > 0 && (
          <div className="mt-5 space-y-2">
            <div className="mb-1 flex items-center justify-between gap-2 text-[11px] text-slate-600">
              <span>{purchaseRows.length} из {shownHistory.length}</span>
              {activeEmail && <span className="truncate">{activeEmail}</span>}
            </div>
            {purchaseRows.length > 0 && (
              <div className="flex flex-wrap items-center gap-2 rounded-xl border border-white/[0.06] bg-black/10 px-3 py-2">
                <Checkbox
                  checked={allVisibleSelected}
                  onChange={() => toggleSelectAllVisible()}
                  disabled={busy || historyLoading || !!historyBatchBusy}
                  aria-label="Выбрать все"
                />
                <span className="text-xs text-slate-400">
                  {selectedCount ? `Выбрано ${selectedCount}` : "Выберите приложения"}
                </span>
                {selectedCount > 0 && (
                  <>
                    <button
                      type="button"
                      className="btn-primary ml-auto px-3 py-1.5 text-[11px]"
                      disabled={busy || historyLoading || !!historyActionId || !!historyBatchBusy}
                      onClick={() => void queueSelectedFromHistory(true)}
                    >
                      {historyBatchBusy === "install" ? (
                        <LoaderCircle size={14} className="animate-spin" />
                      ) : (
                        <Smartphone size={14} />
                      )}
                      На iPhone
                    </button>
                    <button
                      type="button"
                      className="btn-secondary px-3 py-1.5 text-[11px]"
                      disabled={busy || historyLoading || !!historyActionId || !!historyBatchBusy}
                      onClick={() => void queueSelectedFromHistory(false)}
                    >
                      {historyBatchBusy === "export" ? (
                        <LoaderCircle size={14} className="animate-spin" />
                      ) : (
                        <Download size={14} />
                      )}
                      Скачать
                    </button>
                    <button
                      type="button"
                      className="px-2 py-1 text-[11px] text-slate-500 hover:text-white"
                      disabled={!!historyBatchBusy}
                      onClick={() => setSelectedHistoryIds([])}
                    >
                      Снять
                    </button>
                  </>
                )}
              </div>
            )}
            {purchaseRows.length === 0 ? (
              <p className="rounded-xl border border-white/[0.06] bg-black/10 px-4 py-3 text-xs text-slate-500">
                Ничего не найдено
              </p>
            ) : (
              purchaseRows.map(item => {
                const rowKey = item.adamId;
                const selected = selectedHistorySet.has(rowKey);
                const installBusy = historyActionId === `${rowKey}:install`;
                const exportBusy = historyActionId === `${rowKey}:export`;
                const rowBusy = !!historyActionId || !!historyBatchBusy;
                const icon = cacheBustUrl(item.artwork, iconStamp);
                return (
                  <div
                    key={rowKey}
                    className={`flex flex-wrap items-center gap-3 rounded-xl border p-3 sm:flex-nowrap ${
                      selected
                        ? "border-brand-400/35 bg-brand-400/[0.08]"
                        : "border-white/[0.06] bg-black/10"
                    }`}
                  >
                    <Checkbox
                      checked={selected}
                      onChange={() => toggleHistorySelected(rowKey)}
                      disabled={busy || historyLoading || rowBusy}
                      aria-label={`Выбрать ${item.name}`}
                    />
                    <div className="h-12 w-12 shrink-0 overflow-hidden rounded-xl border border-white/10 bg-white/5">
                      {icon ? (
                        <img
                          src={icon}
                          alt=""
                          className="h-full w-full object-cover"
                          loading="lazy"
                          draggable={false}
                        />
                      ) : (
                        <div className="grid h-full w-full place-items-center text-slate-600">
                          <Smartphone size={20} />
                        </div>
                      )}
                    </div>
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-sm font-semibold text-slate-100">{item.name}</div>
                      <div className="mt-0.5 truncate text-[11px] text-slate-500">
                        {[
                          item.artist,
                          item.bundleId
                            && !item.bundleId.startsWith("app.store.")
                            ? item.bundleId
                            : "",
                          item.date ? item.date.slice(0, 10) : "",
                        ]
                          .filter(Boolean)
                          .join(" · ")}
                      </div>
                    </div>
                    <div className="flex w-full shrink-0 flex-wrap gap-2 sm:w-auto sm:flex-nowrap">
                      <button
                        type="button"
                        className="btn-primary flex-1 px-3 py-2 text-[11px] sm:flex-none"
                        disabled={busy || historyLoading || historyLoadingMore || rowBusy}
                        onClick={() => void queueInstallFromHistory(item)}
                      >
                        {installBusy ? (
                          <LoaderCircle size={14} className="animate-spin" />
                        ) : (
                          <Smartphone size={14} />
                        )}
                        На iPhone
                      </button>
                      <button
                        type="button"
                        className="btn-secondary flex-1 px-3 py-2 text-[11px] sm:flex-none"
                        disabled={busy || historyLoading || historyLoadingMore || rowBusy}
                        onClick={() => void queueExportFromHistory(item)}
                      >
                        {exportBusy ? (
                          <LoaderCircle size={14} className="animate-spin" />
                        ) : (
                          <Download size={14} />
                        )}
                        Скачать
                      </button>
                    </div>
                  </div>
                );
              })
            )}
            {historyHasMore && (
              <button
                type="button"
                className="btn-secondary mt-3 w-full py-3 text-xs"
                disabled={historyLoadingMore || historyLoading || historyLoadingAll || busy}
                onClick={() => void loadPurchaseHistory(undefined, { loadMore: true })}
              >
                {historyLoadingMore ? (
                  <LoaderCircle size={14} className="animate-spin" />
                ) : (
                  <Plus size={14} />
                )}
                {historyLoadingMore ? "…" : "Загрузить ещё"}
              </button>
            )}
          </div>
        )}
      </section>

      {                                               }
      {hasAccountProfile && (
        <section id="account-login-form" className="panel mt-5 border-dashed border-white/[0.08] p-6">
          <div className="flex items-center gap-3">
            <div className="rounded-xl bg-white/[0.04] p-3 text-slate-400">
              <Plus size={20} />
            </div>
            <div>
              <h2 className="text-sm font-bold text-slate-200">Добавить Apple ID</h2>
              <p className="mt-0.5 text-xs text-slate-500">
                Ещё один аккаунт для установок и своей истории покупок
              </p>
            </div>
          </div>
          {loginForm}
        </section>
      )}
    </div>
  );
}
