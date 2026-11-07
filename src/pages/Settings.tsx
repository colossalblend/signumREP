import { listen } from "@tauri-apps/api/event";
import { open, save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import { ClipboardCopy, Cloud, Database, FileText, FolderOpen, GraduationCap, Info, KeyRound, LoaderCircle, Monitor, Moon, PackageOpen, Play, RefreshCw, Save, Square, Sun, Trash2, Wrench, Zap } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useBlocker } from "react-router-dom";
import PageHeader from "../components/PageHeader";
import ToggleSwitch from "../components/ToggleSwitch";
import { applyAppearance } from "../lib/appearance";
import { command, errorMessage, setDetailedErrors } from "../lib/tauri";
import { resetOnboarding } from "../lib/tourStore";
import type { AppSettings, AppVersionInfo, CachedIpa, IcloudEnsureResult, IcloudSetupProgress, IpatoolEngine, IpatoolKeychainMode, OtaUpdateInfo } from "../types";

function isWindowsClient(): boolean {
  if (typeof navigator === "undefined") return false;
  const ua = navigator.userAgent || "";
  const platform = (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData
    ?.platform;
  return /Win/i.test(platform || "") || /Windows/i.test(ua) || /Win32|Win64/i.test(navigator.platform);
}

const empty: AppSettings = {
  downloads_dir: "",
  exports_dir: "",
  theme: "dark",
  auto_update: true,
  auto_purchase_new_apps: false,
  auto_resign_before_expiry: false,
  lite_design: false,
  detailed_errors: false,
  auto_diag: true,
  lite_static_loaders: true,
  lite_reduce_motion: true,
  lite_simple_scrolling: true,
  lite_disable_blur: true,
  lite_disable_shadows: true,
  lite_flat_backgrounds: true,
  ipatool_keychain_mode: "random",
  ipatool_engine: "auto",
  background_enabled: false,
  proxy_enabled: false,
  proxy_kind: "socks5",
  proxy_host: "",
  proxy_port: 0,
  proxy_user: "",
  proxy_password: "",
  selected_device_udid: undefined,
  keep_history_session: true,
};

const keychainModes: Array<{ mode: IpatoolKeychainMode; label: string; hint: string }> = [
  { mode: "random", label: "Автоматически", hint: "Случайный ключ создаётся автоматически и хранится в системе" },
  { mode: "custom", label: "Свой ключ", hint: "Укажите свой ключ для защиты сохранённого входа" },
];

const ipatoolEngines: Array<{ mode: IpatoolEngine; label: string; hint: string }> = [
  {
    mode: "auto",
    label: "Авто (рекомендуется)",
    hint: "Signum сам выберет подходящую версию для входа и загрузки приложений.",
  },
  {
    mode: "v5",
    label: "V5",
    hint: "Новая и более стабильная версия для входа и загрузки приложений.",
  },
  {
    mode: "v4",
    label: "V4",
    hint: "Старая версия. Может работать некорректно. Попробуйте её, если с V5 возникли проблемы.",
  },
];

type LiteOptimizationKey =
  | "lite_static_loaders"
  | "lite_reduce_motion"
  | "lite_simple_scrolling"
  | "lite_disable_blur"
  | "lite_disable_shadows"
  | "lite_flat_backgrounds";

const liteOptimizations: Array<{ key: LiteOptimizationKey; label: string; hint: string }> = [
  { key: "lite_static_loaders", label: "Без анимации загрузки", hint: "Отключает вращение индикаторов и движение полосы загрузки" },
  { key: "lite_reduce_motion", label: "Без плавных переходов", hint: "Убирает декоративные анимации и переходы интерфейса" },
  { key: "lite_simple_scrolling", label: "Простая прокрутка", hint: "Переход к нужному месту происходит сразу, без анимации" },
  { key: "lite_disable_blur", label: "Без размытия", hint: "Убирает размытие и прозрачность панелей" },
  { key: "lite_disable_shadows", label: "Без теней и свечения", hint: "Убирает тени и свечение вокруг значков" },
  { key: "lite_flat_backgrounds", label: "Упрощённые фоны", hint: "Использует сплошные цвета без узоров и градиентов" },
];

function settingsSnapshot(s: AppSettings): string {
  return JSON.stringify({
    downloads_dir: s.downloads_dir,
    exports_dir: s.exports_dir,
    theme: s.theme,
    auto_update: s.auto_update,
    auto_purchase_new_apps: !!s.auto_purchase_new_apps,
    auto_resign_before_expiry: !!s.auto_resign_before_expiry,
    lite_design: !!s.lite_design,
    detailed_errors: !!s.detailed_errors,
    auto_diag: s.auto_diag !== false,
    lite_static_loaders: s.lite_static_loaders !== false,
    lite_reduce_motion: s.lite_reduce_motion !== false,
    lite_simple_scrolling: s.lite_simple_scrolling !== false,
    lite_disable_blur: s.lite_disable_blur !== false,
    lite_disable_shadows: s.lite_disable_shadows !== false,
    lite_flat_backgrounds: s.lite_flat_backgrounds !== false,
    ipatool_engine: s.ipatool_engine || "auto",
    background_enabled: !!s.background_enabled,
    proxy_enabled: !!s.proxy_enabled,
    proxy_kind: s.proxy_kind || "socks5",
    proxy_host: s.proxy_host ?? "",
    proxy_port: Number(s.proxy_port) || 0,
    proxy_user: s.proxy_user ?? "",
    proxy_password: s.proxy_password ?? "",
    ipatool_keychain_mode: s.ipatool_keychain_mode === "custom" ? "custom" : "random",
    ipatool_keychain_passphrase: s.ipatool_keychain_mode === "custom" ? (s.ipatool_keychain_passphrase ?? "") : "",
    selected_device_udid: s.selected_device_udid ?? null,
    keep_history_session: s.keep_history_session !== false,
  });
}

export default function Settings() {
  const [settings, setSettings] = useState<AppSettings>(empty);
  const [savedSettings, setSavedSettings] = useState<AppSettings | null>(null);
  const [busy, setBusy] = useState(true);
  const [message, setMessage] = useState("");
  const [cache, setCache] = useState("—");
  const [ota, setOta] = useState<OtaUpdateInfo | null>(null);
  const [otaPercent, setOtaPercent] = useState(0);
  const [appVersion, setAppVersion] = useState<AppVersionInfo>({ version: "1.1.1", build: "17", beta: false });
  const [randomPassphrase, setRandomPassphrase] = useState("");
  const [cachedIpas, setCachedIpas] = useState<CachedIpa[]>([]);
  const [devMenuUnlocked, setDevMenuUnlocked] = useState(() => sessionStorage.getItem("signum-dev-menu") === "1");
  const devMenuClicks = useRef(0);
  // Смена движка обесценивает сохранённую сессию: у каждого свой формат файла
  // аккаунта, и чужую он не прочитает. Поэтому меняем только с подтверждения и
  // сразу разлогиниваем — иначе человек остался бы «залогинен» в интерфейсе,
  // а любая операция падала бы с невнятной ошибкой.
  const [pendingEngine, setPendingEngine] = useState<IpatoolEngine | null>(null);
  const [engineSwitching, setEngineSwitching] = useState(false);
  const [appleStackBusy, setAppleStackBusy] = useState(false);
  const [appleStackPercent, setAppleStackPercent] = useState(0);
  const [appleStackPhase, setAppleStackPhase] = useState("");
  const [diagActive, setDiagActive] = useState(false);
  const [diagHasLog, setDiagHasLog] = useState(false);
  const [diagBusy, setDiagBusy] = useState(false);

  const dirty = useMemo(() => {
    if (!savedSettings) return false;
    return settingsSnapshot(settings) !== settingsSnapshot(savedSettings);
  }, [settings, savedSettings]);

  const dirtyRef = useRef(false);
  dirtyRef.current = dirty;

  function handleVersionClick() {
    if (devMenuUnlocked) return;
    devMenuClicks.current += 1;
    if (devMenuClicks.current >= 7) {
      sessionStorage.setItem("signum-dev-menu", "1");
      setDevMenuUnlocked(true);
      setMessage("Меню разработчика открыто");
    }
  }

  function hideDevMenu() {
    sessionStorage.removeItem("signum-dev-menu");
    devMenuClicks.current = 0;
    setDevMenuUnlocked(false);
  }

  const loadRandomPassphrase = useCallback(async (mode: IpatoolKeychainMode) => {
    if (mode !== "random") {
      setRandomPassphrase("");
      return;
    }
    try {
      setRandomPassphrase(await command<string>("get_ipatool_random_passphrase"));
    } catch {
      setRandomPassphrase("");
    }
  }, []);

  useEffect(() => {
    Promise.all([
      command<AppSettings>("get_settings"),
      command<number>("get_cache_size"),
      command<CachedIpa[]>("list_cached_ipas"),
      command<AppVersionInfo>("get_app_version_info"),
      command<{ active: boolean; hasLog: boolean }>("get_diag_status").catch(() => ({ active: false, hasLog: false })),
    ])
      .then(async ([s, bytes, ipas, versionInfo, diag]) => {
        const mode: IpatoolKeychainMode = s.ipatool_keychain_mode === "custom" ? "custom" : "random";
        const engine: IpatoolEngine = ipatoolEngines.some(e => e.mode === s.ipatool_engine)
          ? s.ipatool_engine
          : "auto";
        const next: AppSettings = { ...s, ipatool_keychain_mode: mode, ipatool_engine: engine };
        setSettings(next);
        setSavedSettings(next);
        setDetailedErrors(s.detailed_errors);
        applyAppearance(s);
        setCache(formatBytes(bytes));
        setCachedIpas(ipas);
        setAppVersion(versionInfo);
        setDiagActive(!!diag.active);
        setDiagHasLog(!!diag.hasLog);
        await loadRandomPassphrase(mode);
      })
      .catch(e => setMessage(errorMessage(e)))
      .finally(() => setBusy(false));
  }, [loadRandomPassphrase]);

  useEffect(() => {
    const unlisten = listen<{ percent: number }>("ota-progress", ({ payload }) => setOtaPercent(payload.percent));
    return () => { void unlisten.then(fn => fn()); };
  }, []);

  useEffect(() => {
    if (!isWindowsClient()) return;
    const unlisten = listen<IcloudSetupProgress>("icloud-setup-progress", ({ payload }) => {
      if (typeof payload.percent === "number") setAppleStackPercent(payload.percent);
      if (payload.message) setAppleStackPhase(payload.message);
      if (payload.phase === "ready" || payload.phase === "failed") {
        setAppleStackBusy(false);
      }
    });
    return () => { void unlisten.then(fn => fn()); };
  }, []);

  async function reinstallAppleStack() {
    if (!isWindowsClient() || appleStackBusy || busy) return;
    const ok = window.confirm(
      "Переустановить драйвер Apple для iPhone?\n\n"
        + "Сначала он будет удалён с компьютера, затем установлен заново. iTunes и iCloud не трогаем.\n"
        + "Если Windows спросит разрешение — нажмите «Да».\n"
        + "После установки полностью закройте Signum и откройте снова.",
    );
    if (!ok) return;
    setAppleStackBusy(true);
    setAppleStackPercent(0);
    setAppleStackPhase("Подготавливаем переустановку драйвера…");
    setMessage("");
    try {
      const result = await command<IcloudEnsureResult>("ensure_icloud_dependency", { force: true });
      if (result.installed || result.status === "ready" || result.status === "installed") {
        setMessage(result.message || "Готово. Полностью закройте Signum и войдите снова.");
        setAppleStackPhase("Готово");
        setAppleStackPercent(100);
      } else {
        setMessage(result.message || "Не удалось завершить. Повторите или перезагрузите компьютер.");
      }
    } catch (e) {
      setMessage(errorMessage(e));
    } finally {
      setAppleStackBusy(false);
    }
  }




  const shouldBlockNav = useCallback(
    ({ currentLocation, nextLocation }: { currentLocation: { pathname: string }; nextLocation: { pathname: string } }) =>
      dirtyRef.current && currentLocation.pathname !== nextLocation.pathname,
    [],
  );
  const blocker = useBlocker(shouldBlockNav);

  useEffect(() => {
    if (blocker.state !== "blocked") return;
    const leave = window.confirm(
      "Изменения в настройках не сохранены.\n\nУйти без сохранения?",
    );
    if (leave) {
      setTimeout(() => blocker.proceed?.(), 0);
    } else {
      blocker.reset?.();
    }
  }, [blocker, blocker.state]);

  useEffect(() => {
    if (blocker.state === "blocked" && !dirtyRef.current) {
      blocker.reset?.();
    }
  }, [blocker, dirty]);


  useEffect(() => {
    if (!dirty) return;
    const onBeforeUnload = (e: BeforeUnloadEvent) => {
      e.preventDefault();
      e.returnValue = "";
    };
    window.addEventListener("beforeunload", onBeforeUnload);
    return () => window.removeEventListener("beforeunload", onBeforeUnload);
  }, [dirty]);

  async function save() {
    setBusy(true);
    try {
      await command<void>("save_settings", { settings });
      setSavedSettings(settings);
      setDetailedErrors(settings.detailed_errors);
      applyAppearance(settings);
      await loadRandomPassphrase(settings.ipatool_keychain_mode);
      setMessage("Настройки сохранены");
    } catch (e) {
      setMessage(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function copyRandomPassphrase() {
    if (!randomPassphrase) return;
    try {
      await navigator.clipboard.writeText(randomPassphrase);
      setMessage("Ключ скопирован");
    } catch (e) {
      setMessage(errorMessage(e));
    }
  }

  async function regenerateRandomPassphrase() {
    setBusy(true);
    try {
      const passphrase = await command<string>("regenerate_ipatool_random_passphrase");
      setRandomPassphrase(passphrase);
      setMessage("Создан новый случайный ключ. Если Apple ID уже подключён — войдите снова.");
    } catch (e) {
      setMessage(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function pickDownloads() {
    const path = await open({ directory: true, multiple: false });
    if (typeof path === "string") setSettings(s => ({ ...s, downloads_dir: path }));
  }

  async function pickExports() {
    const path = await open({ directory: true, multiple: false });
    if (typeof path === "string") setSettings(s => ({ ...s, exports_dir: path }));
  }

  async function clear() {
    setBusy(true);
    try {
      await command<void>("clear_cache");
      const bytes = await command<number>("get_cache_size");
      setCache(formatBytes(bytes));
      setCachedIpas([]);
      setMessage("Кэш очищен");
    } catch (e) {
      setMessage(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function deleteCachedIpa(filename: string) {
    setBusy(true);
    try {
      await command<void>("delete_cached_ipa", { input: { filename } });
      const [bytes, ipas] = await Promise.all([
        command<number>("get_cache_size"),
        command<CachedIpa[]>("list_cached_ipas"),
      ]);
      setCache(formatBytes(bytes));
      setCachedIpas(ipas);
      setMessage("Файл удалён из кэша");
    } catch (e) {
      setMessage(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function installCachedIpa(path: string) {
    setBusy(true);
    try {
      await command<string>("enqueue_local_install", { path });
      setMessage("Файл добавлен в очередь установки");
    } catch (e) {
      setMessage(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function update() {
    setBusy(true);
    try {
      const result = await command<OtaUpdateInfo>("check_for_update");
      setOta(result.has_update ? result : null);
      setMessage(result.has_update
        ? `Доступна версия ${result.version ?? "новее"}`
        : "Установлена последняя версия");
    } catch (e) {
      setMessage(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function installOta() {
    if (!ota?.download_url) return;
    setBusy(true);
    setOtaPercent(0);
    try {
      const path = await command<string>("download_update", {
        url: ota.download_url,
        expectedSha256: ota.sha256,
      });
      await command<void>("install_downloaded_update", { path });
    } catch (e) {
      setMessage(errorMessage(e));
      setBusy(false);
    }
  }

  if (busy && !settings.downloads_dir) {
    return <div className="grid h-full place-items-center"><LoaderCircle className="animate-spin text-brand-300" /></div>;
  }

  return (
    <div className="relative mx-auto max-w-5xl p-7 pb-28 lg:p-9 lg:pb-32">
      <PageHeader title="Настройки" subtitle="Папка загрузок, оформление и обслуживание" />
      {dirty && (
        <div className="mt-5 rounded-xl border border-amber-400/20 bg-amber-400/10 px-4 py-3 text-xs text-amber-100/90">
          Есть несохранённые изменения. Нажмите «Сохранить» внизу справа или они пропадут при выходе.
        </div>
      )}
      {message && <div className="mt-5 rounded-xl border border-white/[0.07] bg-white/[0.035] p-3 text-xs text-slate-300">{message}</div>}

      <section className="panel mt-6 flex flex-wrap items-center justify-between gap-4 p-6">
        <div>
          <h2 className="font-bold">Обучение</h2>
          <p className="mt-1 text-xs leading-5 text-slate-500">
            Покажем, как пользоваться Signum: где найти нужное приложение и как поставить его
            на iPhone. Ничего менять не придётся — просто пройдём по шагам.
          </p>
        </div>
        <button
          type="button"
          className="btn-secondary shrink-0"
          onClick={() => {
            resetOnboarding();
            window.location.reload();
          }}
        >
          <GraduationCap size={15} />
          Пройти заново
        </button>
      </section>

      <section className="panel mt-5 p-6">
        <SectionTitle icon={<FolderOpen />} title="Загрузки" />
        <p className="mt-3 text-xs leading-5 text-slate-500">
          На Windows всё нужное для работы с iPhone уже внутри Signum.
          На Mac при первой установке система может попросить разрешить доступ к устройству.
        </p>
        <label className="mt-4 block">
          <span className="mb-2 block text-xs text-slate-500">Папка загрузок</span>
          <div className="flex gap-2">
            <input className="field" value={settings.downloads_dir} onChange={e => setSettings(s => ({ ...s, downloads_dir: e.target.value }))} />
            <button type="button" className="btn-secondary px-3" onClick={() => void pickDownloads()}><FolderOpen size={16} /></button>
          </div>
        </label>
        <label className="mt-4 block">
          <span className="mb-2 block text-xs text-slate-500">Папка экспорта</span>
          <p className="mb-2 text-[11px] leading-4 text-slate-600">
            Сюда сохраняются файлы приложений из покупок Apple ID — чтобы перенести их на другой компьютер.
          </p>
          <div className="flex gap-2">
            <input className="field" value={settings.exports_dir} onChange={e => setSettings(s => ({ ...s, exports_dir: e.target.value }))} />
            <button type="button" className="btn-secondary px-3" onClick={() => void pickExports()} title="Выбрать папку"><FolderOpen size={16} /></button>
          </div>
        </label>
      </section>

      <section className="panel mt-5 p-6">
        <SectionTitle icon={<KeyRound />} title="Вход и загрузка приложений" />
        <p className="mt-3 text-xs leading-5 text-slate-500">
          Настройки Apple ID на этом компьютере.
        </p>
        <div className="mt-5 border-t border-white/[0.06] pt-5">
          <div className="flex items-center justify-between gap-3">
            <div>
              <div className="text-xs font-semibold text-slate-200">Подготовка загрузок в фоне</div>
              <p className="mt-1 text-[11px] leading-4 text-slate-500">
                Автоматически подготавливает Apple ID для более быстрой загрузки версий.
                Работает раз в час и запускается только в фоне.
              </p>
            </div>
            <ToggleSwitch
              checked={settings.background_enabled}
              onChange={v => setSettings(s => ({ ...s, background_enabled: v }))}
            />
          </div>
        </div>

        <div className="mt-5 border-t border-white/[0.06] pt-5">
          <div className="flex items-center justify-between gap-3">
            <div>
              <div className="text-xs font-semibold text-slate-200">Прокси для Apple</div>
              <p className="mt-1 text-[11px] leading-4 text-slate-500">
                Нужен там, где Apple недоступна напрямую. Работает и для загрузок — VPN
                включать не требуется.
              </p>
            </div>
            <ToggleSwitch
              checked={settings.proxy_enabled}
              onChange={v => setSettings(s => ({ ...s, proxy_enabled: v }))}
            />
          </div>
          {settings.proxy_enabled && (
            <div className="mt-3 space-y-2">
              <div className="flex flex-wrap gap-2">
                {(["socks5", "socks4", "http", "https"] as const).map(kind => (
                  <button
                    key={kind}
                    type="button"
                    onClick={() => setSettings(s => ({ ...s, proxy_kind: kind }))}
                    className={`rounded-lg border px-3 py-1.5 text-[11px] font-semibold transition ${
                      settings.proxy_kind === kind
                        ? "border-brand-400/35 bg-brand-500/10 text-brand-200"
                        : "border-white/[0.08] bg-black/10 text-slate-400"
                    }`}
                  >
                    {kind}
                  </button>
                ))}
              </div>
              <div className="grid gap-2 sm:grid-cols-[1fr_120px]">
                <input
                  className="field"
                  placeholder="Адрес, например 127.0.0.1"
                  value={settings.proxy_host}
                  onChange={e => setSettings(s => ({ ...s, proxy_host: e.target.value.trim() }))}
                />
                <input
                  className="field"
                  placeholder="Порт"
                  inputMode="numeric"
                  value={settings.proxy_port ? String(settings.proxy_port) : ""}
                  onChange={e => setSettings(s => ({ ...s, proxy_port: Number(e.target.value.replace(/\D/g, "")) || 0 }))}
                />
              </div>
              <div className="grid gap-2 sm:grid-cols-2">
                <input
                  className="field"
                  placeholder="Логин (если нужен)"
                  value={settings.proxy_user}
                  onChange={e => setSettings(s => ({ ...s, proxy_user: e.target.value }))}
                />
                <input
                  className="field"
                  type="password"
                  placeholder="Пароль (если нужен)"
                  value={settings.proxy_password}
                  onChange={e => setSettings(s => ({ ...s, proxy_password: e.target.value }))}
                />
              </div>
              <p className="text-[11px] leading-4 text-slate-500">
                Логин и пароль заполняйте, только если прокси их требует. Для socks5 обычно
                хватает адреса и порта. Сохраните настройки и перезапустите Signum,
                чтобы применить прокси.
              </p>
            </div>
          )}
        </div>
        <div className="mt-5 border-t border-white/[0.06] pt-5">
           <div className="flex items-center gap-2 text-xs font-semibold text-slate-200"><Zap size={14} className="text-brand-300" />Режим входа и загрузки</div>
          <p className="mt-1 text-[11px] leading-4 text-slate-500">
            Рекомендуем «Авто». Выберите другую версию, если не удаётся войти или загрузить приложение.
          </p>
          <div className="mt-3 grid gap-2 sm:grid-cols-3">
            {ipatoolEngines.map(({ mode, label, hint }) => (
              <button
                key={mode}
                type="button"
                onClick={() => { if (mode !== settings.ipatool_engine) setPendingEngine(mode); }}
                className={`rounded-xl border p-3 text-left transition ${settings.ipatool_engine === mode ? "border-brand-400/35 bg-brand-500/10 text-brand-200" : "border-white/[0.08] bg-black/10 text-slate-300"}`}
              >
                <div className="text-xs font-semibold">{label}</div>
                <div className="mt-1 text-[10px] leading-4 opacity-70">{hint}</div>
              </button>
            ))}
          </div>
        </div>
        <div className="mt-4 grid grid-cols-2 gap-2">
          {keychainModes.map(({ mode, label, hint }) => (
            <button
              key={mode}
              type="button"
              onClick={() => {
                setSettings(s => ({
                  ...s,
                  ipatool_keychain_mode: mode,
                  ipatool_keychain_passphrase: mode === "custom" ? s.ipatool_keychain_passphrase : undefined,
                }));
                if (mode === "random") void loadRandomPassphrase("random");
                else setRandomPassphrase("");
              }}
              className={`rounded-xl border p-4 text-left transition ${
                settings.ipatool_keychain_mode === mode
                  ? "border-brand-400/35 bg-brand-500/10 text-brand-600 dark:text-brand-200"
                  : "border-subtle bg-black/5 text-muted dark:bg-black/10"
              }`}
            >
              <div className="text-xs font-semibold">{label}</div>
              <div className="mt-2 text-[11px] leading-4 opacity-80">{hint}</div>
            </button>
          ))}
        </div>
        {settings.ipatool_keychain_mode === "random" && (
          <label className="mt-4 block">
            <span className="mb-2 block text-xs text-slate-500">Случайный ключ</span>
            <div className="flex gap-2">
              <input className="field font-mono text-xs" readOnly value={randomPassphrase || "Создаётся при сохранении…"} />
              <button type="button" className="btn-secondary px-3" disabled={!randomPassphrase} onClick={() => void copyRandomPassphrase()} title="Копировать">
                <ClipboardCopy size={16} />
              </button>
              <button type="button" className="btn-secondary px-3" disabled={busy} onClick={() => void regenerateRandomPassphrase()} title="Сгенерировать новый">
                <RefreshCw size={16} />
              </button>
            </div>
            <p className="mt-2 text-[11px] leading-4 text-slate-600">
              Сохраните ключ, если переносите Signum на другой компьютер. Новый ключ потребует повторного входа в Apple ID.
            </p>
          </label>
        )}
        {settings.ipatool_keychain_mode === "custom" && (
          <label className="mt-4 block">
            <span className="mb-2 block text-xs text-slate-500">Свой ключ шифрования</span>
            <input
              className="field"
              type="password"
              value={settings.ipatool_keychain_passphrase ?? ""}
              onChange={e => setSettings(s => ({ ...s, ipatool_keychain_passphrase: e.target.value }))}
            />
          </label>
        )}
      </section>

      <div className="mt-5 grid gap-5 md:grid-cols-2">
        <section className="panel p-6">
          <SectionTitle icon={<Sun />} title="Оформление" />
          <div className="mt-5 grid grid-cols-3 gap-2">
            {([
              ["light", Sun, "Светлая"],
              ["dark", Moon, "Тёмная"],
              ["system", Monitor, "Системная"],
            ] as const).map(([theme, Icon, label]) => (
              <button
                key={theme}
                onClick={() => setSettings(s => ({ ...s, theme }))}
                className={`rounded-xl border p-4 text-left transition ${settings.theme === theme ? "border-brand-400/35 bg-brand-500/10 text-brand-600 dark:text-brand-200" : "border-subtle bg-black/5 text-muted dark:bg-black/10"}`}
              >
                <Icon size={17} />
                <div className="mt-3 text-xs font-semibold">{label}</div>
              </button>
            ))}
          </div>
          <div className="mt-4 flex items-center justify-between gap-4 rounded-xl border border-white/[0.06] bg-black/10 p-4">
            <span className="text-xs font-medium">Упрощённое оформление</span>
            <ToggleSwitch
              checked={settings.lite_design}
              onChange={checked => setSettings(s => ({ ...s, lite_design: checked }))}
              aria-label="Упрощённое оформление"
            />
          </div>
          <p className="mt-2 text-[11px] leading-4 text-slate-600">Можно оставить только нужные оптимизации. Перезапуск не нужен — сохраните настройки.</p>
          {settings.lite_design && (
            <div className="mt-3 rounded-xl border border-white/[0.06] bg-black/10 p-3">
              <div className="mb-2 text-[11px] font-semibold uppercase tracking-wider text-slate-500">Настройки оформления</div>
              <div className="space-y-1.5">
                {liteOptimizations.map(option => (
                  <div key={option.key} className="flex items-start justify-between gap-3 rounded-lg px-2 py-2 hover:bg-white/[0.035]">
                    <span>
                      <span className="block text-xs font-medium text-slate-200">{option.label}</span>
                      <span className="mt-0.5 block text-[10px] leading-4 text-slate-600">{option.hint}</span>
                    </span>
                    <ToggleSwitch
                      className="mt-0.5"
                      checked={settings[option.key]}
                      onChange={checked => setSettings(s => ({ ...s, [option.key]: checked }))}
                      aria-label={option.label}
                    />
                  </div>
                ))}
              </div>
            </div>
          )}
          <div className="mt-4 flex items-center justify-between gap-4 rounded-xl border border-white/[0.06] bg-black/10 p-4">
            <span className="text-xs font-medium">Автопроверка обновлений</span>
            <ToggleSwitch
              checked={settings.auto_update}
              onChange={checked => setSettings(s => ({ ...s, auto_update: checked }))}
              aria-label="Автопроверка обновлений"
            />
          </div>
          <div className="mt-3 flex items-start justify-between gap-4 rounded-xl border border-white/[0.06] bg-black/10 p-4">
            <span className="min-w-0">
              <span className="block text-xs font-medium text-slate-100">Вход в историю покупок</span>
              <span className="mt-1.5 block text-[11px] leading-5 text-slate-500">
                Не нужно каждый раз вводить код, чтобы смотреть историю покупок. Если отключить —
                код могут попросить при каждой проверке.
              </span>
            </span>
            <ToggleSwitch
              className="mt-0.5 shrink-0"
              checked={settings.keep_history_session !== false}
              onChange={checked => setSettings(s => ({ ...s, keep_history_session: checked }))}
              aria-label="Вход в историю покупок"
            />
          </div>
          <div className="mt-3 flex items-start justify-between gap-4 rounded-xl border border-white/[0.06] bg-black/10 p-4">
            <span className="min-w-0">
              <span className="flex flex-wrap items-center gap-2">
                <span className="text-xs font-medium text-slate-100">Автопокупка маскировок</span>
                <span className="inline-flex items-center rounded-md border border-amber-400/25 bg-amber-400/10 px-1.5 py-0.5 text-[9px] font-bold uppercase tracking-wider text-amber-200">
                  Бета
                </span>
              </span>
              <span className="mt-1.5 block text-[11px] leading-5 text-slate-500">
                В фоне покупает на ваш Apple ID новые замаскированные банки и сервисы из App Store.
                Как только в каталоге появляется свежая маскировка — Signum тихо привязывает её к
                аккаунту; iPhone не нужен, поставить можно позже. Нужен вход в Apple ID.
              </span>
            </span>
            <ToggleSwitch
              className="mt-0.5 shrink-0"
              checked={!!settings.auto_purchase_new_apps}
              onChange={checked => setSettings(s => ({ ...s, auto_purchase_new_apps: checked }))}
              aria-label="Автопокупка маскировок (бета)"
            />
          </div>
          <div className="mt-3 flex items-start justify-between gap-4 rounded-xl border border-white/[0.06] bg-black/10 p-4">
            <span className="min-w-0">
              <span className="flex flex-wrap items-center gap-2">
                <span className="text-xs font-medium text-slate-100">Продлевать подпись самому</span>
              </span>
              <span className="mt-1.5 block text-[11px] leading-5 text-slate-500">
                Приложение, подписанное бесплатным Apple ID, перестаёт открываться через неделю.
                С этой настройкой Signum подпишет и поставит его заново за пару дней до этого —
                нужен подключённый iPhone и тот же файл, которым подписывали.
              </span>
            </span>
            <ToggleSwitch
              className="mt-0.5 shrink-0"
              checked={!!settings.auto_resign_before_expiry}
              onChange={checked => setSettings(s => ({ ...s, auto_resign_before_expiry: checked }))}
              aria-label="Продлевать подпись самому"
            />
          </div>
        </section>

        <section className="panel p-6">
          <SectionTitle icon={<Database />} title="Обслуживание" />
          <div className="mt-5 rounded-xl border border-white/[0.06] bg-black/10 p-4">
            <div className="text-[11px] text-slate-600">Кэш загрузок</div>
            <div className="mt-1 text-xl font-bold">{cache}</div>
          </div>
          <div className="mt-3 max-h-64 space-y-2 overflow-y-auto pr-1">
            {cachedIpas.length === 0 ? (
              <div className="rounded-xl border border-white/[0.05] bg-black/10 p-4 text-center text-xs text-slate-600">
                Сохранённых файлов пока нет
              </div>
            ) : cachedIpas.map(ipa => (
              <div key={ipa.filename} className="flex items-center gap-3 rounded-xl border border-white/[0.06] bg-black/10 p-3">
                <PackageOpen size={18} className="shrink-0 text-brand-300" />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-xs font-semibold text-slate-200">{ipa.name ?? ipa.filename}</div>
                  {ipa.apple_id ? (
                    <div className="mt-0.5 truncate text-[10px] font-medium text-brand-300/90" title={`Скачано с Apple ID: ${ipa.apple_id}`}>
                      Apple ID: {ipa.apple_id}
                    </div>
                  ) : null}
                  <div className="mt-1 truncate text-[10px] text-slate-600">
                    {ipa.version ? `v${ipa.version} · ` : ""}{formatBytes(ipa.size)}{ipa.bundle_id ? ` · ${ipa.bundle_id}` : ""}
                    {ipa.device_compatible === true ? " · подходит" : ipa.device_compatible === false ? " · iOS ниже" : ""}
                  </div>
                </div>
                <button className="btn-secondary px-2.5 py-2" disabled={busy} title="Установить" onClick={() => void installCachedIpa(ipa.path)}>
                  <Play size={13} />
                </button>
                <button className="btn-secondary px-2.5 py-2 text-rose-300" disabled={busy} title="Удалить" onClick={() => void deleteCachedIpa(ipa.filename)}>
                  <Trash2 size={13} />
                </button>
              </div>
            ))}
          </div>
          {ota && (
            <div className="mt-3 rounded-xl border border-brand-400/20 bg-brand-500/10 p-4">
              <div className="text-sm font-semibold text-brand-200">Signum {ota.version ?? "новее"}</div>
              <p className="mt-1 text-xs text-slate-500">{ota.notes ?? "Доступно обновление приложения."}</p>
              {busy && otaPercent > 0 && (
                <div className="mt-3 h-1.5 overflow-hidden rounded-full bg-white/[0.06]">
                  <div className="h-full rounded-full bg-brand-gradient transition-all" style={{ width: `${otaPercent}%` }} />
                </div>
              )}
              <button className="btn-primary mt-3 w-full" disabled={busy || !ota.download_url} onClick={() => void installOta()}>
                <RefreshCw size={15} />Скачать и установить
              </button>
            </div>
          )}
          <div className="mt-3 flex gap-2">
            <button className="btn-secondary flex-1" onClick={() => void clear()}><Trash2 size={15} />Очистить</button>
            <button className="btn-secondary flex-1" onClick={() => void update()}><RefreshCw size={15} />Проверить</button>
          </div>
          {isWindowsClient() && (
            <div className="mt-3 rounded-xl border border-white/[0.06] bg-black/10 p-4">
              <div className="flex items-start gap-3">
                <div className="grid h-9 w-9 shrink-0 place-items-center rounded-xl border border-sky-400/20 bg-sky-400/10 text-sky-300">
                  <Cloud size={17} />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="text-xs font-semibold text-slate-100">Драйвер Apple для iPhone</div>
                  <p className="mt-1 text-[11px] leading-5 text-slate-500">
                    Нужен, чтобы Windows видела iPhone по USB. Signum ставит только его, без iTunes;
                    подойдут и уже установленные iTunes или «Устройства Apple». iCloud не нужен.
                    Если iPhone не виден — нажмите «Переустановить».
                  </p>
                  {appleStackBusy && (
                    <div className="mt-3">
                      <div className="h-1.5 overflow-hidden rounded-full bg-white/10">
                        <div
                          className="h-full rounded-full bg-sky-400 transition-all duration-300"
                          style={{ width: `${Math.max(4, Math.min(100, appleStackPercent))}%` }}
                        />
                      </div>
                      <div className="mt-1.5 text-[10px] text-slate-500">
                        {appleStackPhase || "Идёт установка…"}
                      </div>
                    </div>
                  )}
                  <button
                    type="button"
                    className="btn-secondary mt-3 w-full"
                    disabled={busy || appleStackBusy}
                    onClick={() => void reinstallAppleStack()}
                  >
                    {appleStackBusy ? (
                      <>
                        <LoaderCircle size={15} className="animate-spin" />
                        Устанавливаем…
                      </>
                    ) : (
                      <>
                        <RefreshCw size={15} />
                        Переустановить
                      </>
                    )}
                  </button>
                </div>
              </div>
            </div>
          )}
        </section>
      </div>

      <section className="panel mt-5 p-6">
        <SectionTitle icon={<FileText />} title="Запись для поддержки" />
        <p className="mt-3 text-xs leading-5 text-slate-500">
          Если проблему нужно показать прямо сейчас — включите запись вручную, повторите её и отправьте отчёт.
          Если отправка не выйдет — сохраните отчёт на компьютер.
        </p>
        <div className="mt-4 flex flex-wrap gap-2">
          <button
            type="button"
            className="btn-secondary flex-1"
            disabled={busy || diagBusy || diagActive}
            onClick={() => {
              if (!window.confirm("Signum перезапустится и начнёт запись. Продолжить?")) return;
              setDiagBusy(true);
              void command("start_diag_recording").catch(e => {
                setDiagBusy(false);
                setMessage(errorMessage(e));
              });
            }}
          >
            {diagBusy && !diagActive ? <LoaderCircle size={16} className="animate-spin" /> : <Play size={16} />}
            Запустить
          </button>
          <button
            type="button"
            className="btn-secondary flex-1"
            disabled={busy || diagBusy || !diagActive}
            onClick={async () => {
              setDiagBusy(true);
              try {
                await command("stop_diag_recording");
                setDiagActive(false);
                setMessage("Запись остановлена. Отчёт на компьютере — его можно отправить или сохранить.");
              } catch (e) {
                setMessage(errorMessage(e));
              } finally {
                setDiagBusy(false);
              }
            }}
          >
            <Square size={16} />
            Остановить
          </button>
          <button
            type="button"
            className="btn-primary flex-1"
            disabled={busy || diagBusy || (!diagActive && !diagHasLog)}
            onClick={async () => {
              setDiagBusy(true);
              try {
                await command("send_diag_log");
                setDiagActive(false);
                setDiagHasLog(false);
                setMessage("Отчёт отправлен. Запись выключена.");
              } catch (e) {
                setMessage(`${errorMessage(e)} Можно сохранить отчёт на компьютер.`);
              } finally {
                setDiagBusy(false);
              }
            }}
          >
            {diagBusy && diagActive ? <LoaderCircle size={16} className="animate-spin" /> : <Cloud size={16} />}
            Отправить отчёт
          </button>
          <button
            type="button"
            className="btn-secondary w-full"
            disabled={busy || diagBusy || (!diagActive && !diagHasLog)}
            onClick={async () => {
              setDiagBusy(true);
              try {
                const path = await saveFileDialog({
                  defaultPath: "signum-report.txt",
                  filters: [{ name: "Текст", extensions: ["txt"] }],
                });
                if (!path) return;
                await command("export_diag_log", { path });
                setMessage("Отчёт сохранён на компьютер.");
              } catch (e) {
                setMessage(errorMessage(e));
              } finally {
                setDiagBusy(false);
              }
            }}
          >
            <Save size={16} />
            Скачать отчёт
          </button>
        </div>
      </section>

      <section className="panel mt-5 p-6">
        <SectionTitle icon={<Info />} title="О приложении" />
        <p className="mt-4 text-xs leading-6 text-slate-500">
          Приложение сделано на основе IPA DOWNLOADER —{" "}
          <a
            href="https://github.com/kda2495/IPA_Downloader"
            target="_blank"
            rel="noreferrer"
            className="font-semibold text-brand-300 underline decoration-brand-400/40 underline-offset-4 transition hover:text-brand-200"
          >
            kda2495/IPA_Downloader
          </a>
        </p>
        <button
          type="button"
          onClick={handleVersionClick}
          className="mt-3 cursor-default text-[10px] text-slate-700"
          tabIndex={-1}
        >
          Signum {appVersion.version} · build {appVersion.build}
        </button>
      </section>

      {devMenuUnlocked && (
        <section className="panel mt-5 p-6">
          <SectionTitle icon={<Wrench />} title="Меню разработчика" />
          <div className="mt-5 flex items-start justify-between gap-4 rounded-xl border border-amber-400/15 bg-amber-400/[0.05] p-4">
            <span>
              <span className="block text-xs font-medium text-slate-200">Подробные ошибки</span>
              <span className="mt-1 block text-[10px] leading-4 text-amber-300/80">Показывает подробный лог ошибок. Может содержать служебные данные.</span>
            </span>
            <ToggleSwitch
              className="mt-0.5"
              checked={settings.detailed_errors}
              onChange={checked => setSettings(s => ({ ...s, detailed_errors: checked }))}
              aria-label="Подробные ошибки"
            />
          </div>
          <button type="button" className="btn-secondary mt-3 w-full" onClick={hideDevMenu}>Скрыть меню разработчика</button>
        </section>
      )}


      <button
        type="button"
        className={`signum-fab-save ${dirty ? "is-dirty" : ""}`}
        disabled={busy || !dirty}
        onClick={() => void save()}
        title={dirty ? "Сохранить настройки" : "Изменений нет"}
      >
        {busy ? <LoaderCircle size={16} className="animate-spin" /> : <Save size={16} />}
        {busy ? "Сохранение…" : dirty ? "Сохранить" : "Сохранено"}
      </button>

      {pendingEngine && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4">
          <div className="w-full max-w-md rounded-2xl border border-white/[0.08] bg-[#12131a] p-5">
            <div className="text-base font-semibold text-white">Смена версии выведет из Apple ID</div>
            <p className="mt-2 text-xs leading-5 text-slate-400">
              После смены версии потребуется снова войти в Apple ID.
              Подготовьте пароль и устройство для получения кода подтверждения.
            </p>
            <p className="mt-2 text-xs leading-5 text-slate-400">
              Загруженные файлы и настройки останутся на месте.
            </p>
            <div className="mt-4 flex gap-2">
              <button
                type="button"
                className="btn-primary flex-1"
                disabled={engineSwitching}
                onClick={() => {
                  const next = pendingEngine;
                  setEngineSwitching(true);
                  setMessage("");
                  void (async () => {
                    try {
                      // Выходим до сохранения выбора: если выход не удался,
                      // движок остаётся прежним и сессия ему по-прежнему своя.
                      await command("logout_apple");
                      setSettings(s => ({ ...s, ipatool_engine: next }));
                      setMessage("Версия изменена. Войдите в Apple ID заново на странице «Аккаунт».");
                    } catch (e) {
                      setMessage(errorMessage(e));
                    } finally {
                      setEngineSwitching(false);
                      setPendingEngine(null);
                    }
                  })();
                }}
              >
                {engineSwitching ? <LoaderCircle size={15} className="animate-spin" /> : null}
                {engineSwitching ? "Выходим…" : "Продолжить"}
              </button>
              <button
                type="button"
                className="btn-secondary flex-1"
                disabled={engineSwitching}
                onClick={() => setPendingEngine(null)}
              >
                Отмена
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function SectionTitle({ icon, title }: { icon: React.ReactNode; title: string }) {
  return <div className="flex items-center gap-2.5 text-sm font-bold text-slate-200"><span className="text-brand-300">{icon}</span>{title}</div>;
}

function formatBytes(bytes: number) {
  if (!bytes) return "0 Б";
  const units = ["Б", "КБ", "МБ", "ГБ"];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** i).toFixed(i ? 1 : 0)} ${units[i]}`;
}
