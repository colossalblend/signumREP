import { BatteryCharging, Cable, CheckCircle2, Download, HardDrive, List, LoaderCircle, RefreshCw, Search, ShieldCheck, Smartphone, Wifi, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { deviceModel, deviceDisplayName, deviceSubtitle } from "../lib/deviceNames";
import AppTransfer from "../components/AppTransfer";
import PageHeader from "../components/PageHeader";
import { command, errorMessage } from "../lib/tauri";
import type { BulkInstallResult, DeviceInfo, InstalledApp } from "../types";

function connectionLabel(connection?: string): string {
  switch ((connection || "usb").toLowerCase()) {
    case "wifi":
    case "network":
      return "Wi‑Fi";
    case "both":
      return "USB + Wi‑Fi";
    default:
      return "USB";
  }
}

function connectionIsWifi(connection?: string): boolean {
  const c = (connection || "").toLowerCase();
  return c === "wifi" || c === "network" || c === "both";
}

export default function Devices() {
  const navigate = useNavigate();
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [selectedUdid, setSelectedUdid] = useState<string | null>(null);
  const [device, setDevice] = useState<DeviceInfo | null>(null);
  const [apps, setApps] = useState<InstalledApp[]>([]);
  const [loading, setLoading] = useState(true);
  const [appsLoading, setAppsLoading] = useState(false);
  const [appsLoaded, setAppsLoaded] = useState(false);
  const [message, setMessage] = useState("");
  const [appSearch, setAppSearch] = useState("");
  const [showOffloadedOnly, setShowOffloadedOnly] = useState(false);
  const [restoreBusy, setRestoreBusy] = useState<string | null>(null);

  const refreshDevices = useCallback(async (showLoading = true) => {
    if (showLoading) setLoading(true);
    setMessage("");
    try {
      const [list, settings] = await Promise.all([
        command<DeviceInfo[]>("list_connected_devices"),
        command<{ selected_device_udid?: string }>("get_settings"),
      ]);
      setApps([]);
      setAppsLoaded(false);
      setDevices(list);
      const savedUdid = settings.selected_device_udid?.trim();
      const selected = (savedUdid ? list.find(item => item.udid === savedUdid) : undefined) ?? list[0] ?? null;
      setSelectedUdid(selected?.udid ?? null);
      setDevice(selected);
      if (selected && selected.udid !== savedUdid) {
        void command("select_device", { input: { udid: selected.udid } }).catch(() => undefined);
      }
    } catch (e) {
      setDevices([]);
      setDevice(null);
      setMessage(errorMessage(e));
    } finally {
      setLoading(false);
    }
  }, []);

  const selectDevice = useCallback(async (udid: string) => {
    setMessage("");
    try {
      const picked = await command<DeviceInfo>("select_device", { input: { udid } });
      setSelectedUdid(picked.udid);
      setDevice(picked);
      setApps([]);
      setAppsLoaded(false);
      setMessage(`Выбрано: ${deviceModel(picked)}`);
    } catch (e) {
      setMessage(errorMessage(e));
    }
  }, []);

  const loadApps = useCallback(async () => {
    setAppsLoading(true);
    setMessage("");
    try {
      setApps(await command<InstalledApp[]>("list_installed_apps"));
      setAppsLoaded(true);
    } catch (e) {
      setApps([]);
      setMessage(errorMessage(e));
    } finally {
      setAppsLoading(false);
    }
  }, []);

  const refresh = useCallback(async () => {
    setApps([]);
    setAppsLoaded(false);
    await refreshDevices();
  }, [refreshDevices]);

  const visibleApps = apps.filter(app => {
    const query = appSearch.trim().toLowerCase();
    if (showOffloadedOnly && !app.is_placeholder) return false;
    return !query || `${app.name ?? ""} ${app.bundle_id} ${app.version ?? ""} ${app.app_store_id ?? ""}`.toLowerCase().includes(query);
  }).sort((a, b) => (a.name ?? a.bundle_id).localeCompare(b.name ?? b.bundle_id, "ru"));

  const offloadedApps = useMemo(() => apps.filter(app => app.is_placeholder), [apps]);

  async function restore(items: InstalledApp[]) {
    if (!items.length || restoreBusy) return;
    setRestoreBusy(items.length === 1 ? items[0].bundle_id : "all");
    setMessage("");
    try {
      const result = await command<BulkInstallResult>("enqueue_placeholder_restore", { input: {
        items: items.map(app => ({
          title: app.name || app.bundle_id,
          bundle_id: app.bundle_id,
          app_store_id: app.app_store_id || null,
        })),
      }});
      if (result.jobIds.length) navigate("/installs");
      else setMessage(result.skipped[0]?.reason || "Не удалось добавить сгруженные приложения в очередь.");
    } catch (e) {
      setMessage(errorMessage(e));
    } finally {
      setRestoreBusy(null);
    }
  }

  useEffect(() => { void refreshDevices(); }, [refreshDevices]);

  async function trust() {
    setMessage(
      "Разблокируйте iPhone. Если появится запрос — нажмите «Доверять» и введите код-пароль. Ждём подтверждение…",
    );
    try {
      setMessage(await command<string>("trust_device"));
      await refresh();
    } catch (e) {
      setMessage(errorMessage(e));
    }
  }

  return (
    <div className="mx-auto max-w-6xl p-7 lg:p-9">
      <PageHeader
        title="Устройства"
        subtitle="Выберите iPhone, если подключено несколько"
        actions={<button className="btn-secondary" onClick={() => void refresh()}><RefreshCw size={15} />Обновить</button>}
      />

      {loading ? (
        <div className="panel mt-8 flex min-h-80 flex-col items-center justify-center gap-3">
          <LoaderCircle className="animate-spin text-brand-400" />
          <div className="text-sm text-slate-400">Ищем iPhone…</div>
          <div className="max-w-xs text-center text-xs leading-5 text-slate-500">
            Подключите телефон кабелем и разблокируйте экран
          </div>
        </div>
      ) : devices.length === 0 ? (
        <div className="panel mt-8 flex min-h-80 flex-col items-center justify-center px-8 text-center">
          <div className="rounded-2xl bg-white/5 p-4 text-slate-500"><Cable size={34} /></div>
          <h2 className="mt-4 font-bold">iPhone не найден</h2>
          <p className="mt-2 max-w-md text-sm text-slate-500">
            {message || "Подключите устройство разблокированным кабелем USB."}
          </p>
          <button className="btn-primary mt-5" onClick={() => void trust()}><ShieldCheck size={16} />Доверять устройству</button>
        </div>
      ) : (
        <>
          {devices.length > 1 && (
            <section className="panel mt-8 p-4">
              <h2 className="text-sm font-bold text-slate-200">Подключённые iPhone</h2>
              <div className="mt-3 grid gap-2 md:grid-cols-2">
                {devices.map(item => (
                  <button
                    key={item.udid}
                    type="button"
                    onClick={() => void selectDevice(item.udid)}
                    className={`flex items-center gap-3 rounded-xl border p-4 text-left transition ${
                      selectedUdid === item.udid
                        ? "border-emerald-400/25 bg-emerald-400/[0.05]"
                        : "border-white/[0.06] bg-black/10 hover:border-white/[0.12]"
                    }`}
                  >
                    <Smartphone size={18} className="shrink-0 text-brand-300" />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-sm font-semibold">{deviceDisplayName(item, devices)}</div>
                      <div className="mt-0.5 text-[11px] text-slate-500">
                        {deviceSubtitle(item) || "Подключено"}
                      </div>
                    </div>
                    <span className="shrink-0 rounded-lg bg-white/5 px-2 py-1 text-[10px] font-semibold text-slate-400">
                      {connectionLabel(item.connection)}
                    </span>
                  </button>
                ))}
              </div>
            </section>
          )}

          {device && (
            <section className="panel relative mt-5 overflow-hidden p-6">
              <div className="absolute -right-12 -top-24 h-64 w-64 rounded-full bg-brand-500/10 blur-3xl" />
              <div className="relative flex flex-wrap items-center gap-6">
                <div className="grid h-24 w-20 place-items-center rounded-[24px] border border-white/10 bg-gradient-to-b from-slate-700/60 to-slate-900 shadow-icon">
                  <Smartphone size={42} className="text-brand-300" />
                </div>
                <div className="min-w-56 flex-1">
                  <div className="flex items-center gap-2">
                    <h2 className="text-xl font-bold">{deviceDisplayName(device, devices)}</h2>
                    <CheckCircle2 size={17} className="text-emerald-400" />
                  </div>
                  <p className="mt-1 truncate text-xs text-slate-500">{deviceSubtitle(device) || "Подключено"}</p>
                  <div className="mt-5 grid grid-cols-2 gap-3 lg:grid-cols-4">
                    <Metric icon={<Smartphone />} label="Модель" value={deviceModel(device)} />
                    <Metric icon={<HardDrive />} label="Система" value={device.product_version ? `iOS ${device.product_version}` : "—"} />
                    <Metric icon={<BatteryCharging />} label="Заряд" value={device.battery_current_capacity != null ? `${device.battery_current_capacity}%` : "—"} />
                    <Metric
                      icon={connectionIsWifi(device.connection) ? <Wifi /> : <Cable />}
                      label="Подключение"
                      value={connectionLabel(device.connection)}
                    />
                  </div>
                </div>
                <button className="btn-secondary" onClick={() => void trust()}><ShieldCheck size={16} />Переподключить доверие</button>
              </div>
            </section>
          )}

          {message && <p className="mt-4 rounded-xl border border-white/[0.06] bg-white/[0.03] p-3 text-xs text-slate-400">{message}</p>}

          <AppTransfer devices={devices} onRefresh={() => refreshDevices(false)} />

          <section className="mt-8">
            <div className="mb-4 flex items-center justify-between gap-3">
              <h2 className="font-bold">Установленные приложения</h2>
              {appsLoaded && <div className="flex items-center gap-3 text-xs text-slate-500">
                {offloadedApps.length > 0 && <button type="button" className="text-amber-200 hover:text-amber-100" onClick={() => void restore(offloadedApps)} disabled={!!restoreBusy}>
                  {restoreBusy === "all" ? "Добавляем…" : `Восстановить сгруженные · ${offloadedApps.length}`}
                </button>}
                <span>{apps.length} найдено</span>
              </div>}
            </div>
            {!appsLoaded ? (
              <div className="panel flex min-h-40 flex-col items-center justify-center gap-4 p-8 text-center">
                <p className="max-w-md text-sm text-slate-500">Список приложений не загружается автоматически — это ускоряет открытие страницы.</p>
                <button className="btn-primary" disabled={appsLoading} onClick={() => void loadApps()}>
                  {appsLoading ? <LoaderCircle size={16} className="animate-spin" /> : <List size={16} />}
                  {appsLoading ? "Загрузка…" : "Показать приложения"}
                </button>
              </div>
            ) : appsLoading ? (
              <div className="panel flex min-h-40 flex-col items-center justify-center gap-3">
                <LoaderCircle className="animate-spin text-brand-400" />
                <div className="text-sm text-slate-400">Читаем список приложений…</div>
              </div>
            ) : apps.length === 0 ? (
              <div className="panel flex min-h-40 items-center justify-center p-8 text-sm text-slate-500">Приложения не найдены</div>
            ) : (
              <>
                <div className="panel mb-3 flex flex-wrap items-center gap-3 px-4 py-3 transition focus-within:border-brand-400/40">
                  <Search className="shrink-0 text-brand-300" size={17} />
                  <input value={appSearch} onChange={event => setAppSearch(event.target.value)} placeholder="Поиск по приложениям" aria-label="Поиск по установленным приложениям" className="min-w-0 flex-1 bg-transparent text-sm text-theme outline-none placeholder:text-muted" />
                  {appSearch && <button type="button" onClick={() => setAppSearch("")} aria-label="Очистить поиск" className="text-slate-500 hover:text-white"><X size={15} /></button>}
                  <button type="button" className={`rounded-lg px-2.5 py-1.5 text-[11px] font-semibold ${showOffloadedOnly ? "bg-amber-400/15 text-amber-200" : "bg-white/[0.05] text-slate-500 hover:text-white"}`} onClick={() => setShowOffloadedOnly(value => !value)}>
                    Сгруженные
                  </button>
                </div>
                <div className="grid gap-3 md:grid-cols-2">
                {visibleApps.map(app => (
                    <div key={app.bundle_id} className="panel flex items-center justify-between gap-3 p-4">
                      <div className="min-w-0">
                        <div className="flex items-center gap-2 text-sm font-semibold">
                          <span className="truncate">{app.name ?? app.bundle_id}</span>
                          {app.is_placeholder && <span className="shrink-0 rounded-md bg-amber-400/10 px-1.5 py-0.5 text-[10px] font-semibold text-amber-200">Сгружено</span>}
                        </div>
                        <div className="mt-1 truncate text-xs text-slate-600">{app.bundle_id}{app.app_store_id ? ` · ${app.app_store_id}` : ""}</div>
                      </div>
                      <div className="flex shrink-0 items-center gap-2">
                        <span className="rounded-lg bg-white/5 px-2.5 py-1 text-xs text-slate-400">{app.version ?? "—"}</span>
                        {app.is_placeholder && <button type="button" className="btn-secondary px-2.5 py-1.5 text-[11px]" disabled={!!restoreBusy} onClick={() => void restore([app])}>
                          {restoreBusy === app.bundle_id ? <LoaderCircle size={13} className="animate-spin" /> : <Download size={13} />} Восстановить
                        </button>}
                      </div>
                  </div>
                ))}
                </div>
                {visibleApps.length === 0 && <div className="panel p-6 text-center text-sm text-slate-500">Ничего не найдено</div>}
              </>
            )}
          </section>
        </>
      )}
    </div>
  );
}

function Metric({ icon, label, value }: { icon: React.ReactElement; label: string; value: string }) {
  return (
    <div className="rounded-xl border border-white/[0.055] bg-black/15 p-3">
      <div className="flex items-center gap-1.5 text-[11px] text-slate-500">
        <span className="text-slate-500">{icon}</span>
        {label}
      </div>
      <div className="mt-1 text-sm font-semibold text-slate-200">{value}</div>
    </div>
  );
}
