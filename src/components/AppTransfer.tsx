import { ArrowRight, ArrowRightLeft, Check, CheckCircle2, Download, LayoutGrid, LoaderCircle, RefreshCw, Search, Smartphone, X } from "lucide-react";
import { useRef, useState } from "react";
import { Link } from "react-router-dom";
import Checkbox from "./Checkbox";
import SelectMenu from "./SelectMenu";
import { deviceDisplayName, deviceSubtitle } from "../lib/deviceNames";
import { command, errorMessage } from "../lib/tauri";
import type { BulkInstallResult, DeviceInfo, TransferPreview } from "../types";

const statusLabels = { ready: "Можно перенести", installed: "Уже установлено", unavailable: "Недоступно", incompatible: "Не подходит iOS" };

export default function AppTransfer({ devices, onRefresh }: { devices: DeviceInfo[]; onRefresh: () => Promise<void> }) {
  const [open, setOpen] = useState(false);
  const [source, setSource] = useState("");
  const [target, setTarget] = useState("");
  const [preview, setPreview] = useState<TransferPreview | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState<"preview" | "enqueue" | null>(null);
  const [error, setError] = useState("");
  const [result, setResult] = useState<BulkInstallResult | null>(null);
  const running = useRef(false);
  const ready = preview?.items.flatMap((row, index) => row.status === "ready" ? [index] : []) ?? [];
  const connected = devices.some(d => d.udid === source) && devices.some(d => d.udid === target);
  const sourceDevice = devices.find(device => device.udid === source);
  const targetDevice = devices.find(device => device.udid === target);
  const step = result || busy === "enqueue" ? 2 : preview || busy === "preview" ? 1 : 0;
  const steps = [
    { title: "Устройства", subtitle: "Откуда → куда", icon: Smartphone },
    { title: "Приложения", subtitle: busy === "preview" ? "Проверяем доступность…" : "Все или только нужные", icon: LayoutGrid },
    { title: "Установка", subtitle: result ? "Результат добавления в очередь" : "На новый iPhone", icon: Download },
  ];

  function reset() { setPreview(null); setResult(null); setSelected(new Set()); setError(""); setQuery(""); }

  async function inspect() {
    if (running.current || !connected || source === target) return;
    running.current = true;
    reset(); setBusy("preview");
    try {
      const data = await command<TransferPreview>("preview_app_transfer", { sourceUdid: source, targetUdid: target });
      setPreview(data);
      setSelected(new Set(data.items.flatMap((row, index) => row.status === "ready" ? [index] : [])));
    } catch (e) { setError(errorMessage(e)); }
    finally { running.current = false; setBusy(null); }
  }

  async function transfer() {
    if (running.current || !preview || !selected.size || !connected) return;
    running.current = true;
    setBusy("enqueue"); setError("");
    try {
      const data = await command<BulkInstallResult>("enqueue_app_transfer", { input: {
        target_udid: preview.target_udid,
        items: preview.items.filter((_, index) => selected.has(index)).map(row => row.item),
      } });
      setResult(data);
      setPreview(null);
      await onRefresh();
    } catch (e) { setError(errorMessage(e)); }
    finally { running.current = false; setBusy(null); }
  }

  return <section className="panel relative mt-6 overflow-hidden p-5">
    <div className="pointer-events-none absolute -right-16 -top-20 h-56 w-56 rounded-full bg-brand-500/10 blur-3xl" />
    <div className="relative flex flex-wrap items-center gap-4">
      <div className="rounded-2xl bg-brand-400/10 p-3 text-brand-300"><ArrowRightLeft size={24} /></div>
      <div className="min-w-56 flex-1">
        <h2 className="font-bold">Приложения на новый iPhone</h2>
        <p className="mt-1 text-xs leading-5 text-muted">Выберите два устройства и перенесите все доступные приложения или только нужные.</p>
      </div>
      <button className="btn-primary" aria-expanded={open} aria-controls="app-transfer" disabled={!!busy} onClick={() => setOpen(!open)}>
        {open ? <X size={15} /> : <ArrowRight size={15} />}{open ? "Свернуть" : "Перенести приложения"}
      </button>
    </div>
    <div className={`transfer-reveal ${open ? "is-open" : ""}`} aria-hidden={!open}>
      <div className="min-h-0 overflow-hidden" id="app-transfer">
        <div className="mt-5 border-t border-white/[0.06] pt-5">
          <ol className="transfer-steps" aria-label="Этапы переноса">
            {steps.map(({ title, subtitle, icon: Icon }, index) => <li key={title} className={`transfer-step ${index === step ? "is-current" : index < step ? "is-complete" : ""}`} aria-current={index === step ? "step" : undefined}>
              <span className="transfer-step-symbol"><span key={`${index}-${index < step ? "done" : "pending"}`} className="transfer-step-glyph">{index < step ? <Check size={19} /> : busy && index === step ? <LoaderCircle size={19} className="animate-spin" /> : <Icon size={19} />}</span></span>
              <span className="min-w-0"><span className="transfer-step-title"><span className="transfer-step-number">0{index + 1}</span>{title}</span><span className="transfer-step-subtitle">{subtitle}</span></span>
              <span className="transfer-step-line" />
            </li>)}
          </ol>
          <div className="grid items-end gap-3 md:grid-cols-[1fr_auto_1fr]">
            {(["source", "target"] as const).map((kind, index) => <div key={kind} className="contents">
              {index === 1 && <ArrowRight className="hidden self-center text-brand-300 md:block" size={20} />}
              <div className={`transfer-device-card ${kind === "source" ? "is-source" : "is-target"}`}>
                <p className="transfer-device-role"><span className="transfer-device-dot" />{kind === "source" ? "Старое устройство" : "Новое устройство"}</p>
                <p className="mb-3 mt-1 text-xs text-muted">{kind === "source" ? "Отсюда берём список приложений" : "Сюда установим приложения"}</p>
                <SelectMenu
                  value={kind === "source" ? source : target}
                  disabled={!!busy}
                  placeholder={kind === "source" ? "Выберите старый iPhone" : "Выберите новый iPhone"}
                  aria-label={kind === "source" ? "Старый iPhone · откуда" : "Новый iPhone · куда"}
                  searchPlaceholder="Найти iPhone…"
                  icon={<Smartphone size={18} />}
                  className="device-select-trigger"
                  options={devices.map(device => {
                    const unavailable = device.udid === (kind === "source" ? target : source);
                    const connection = ["wifi", "network"].includes(device.connection ?? "") ? "Wi‑Fi" : device.connection === "both" ? "USB + Wi‑Fi" : "USB";
                    return {
                      value: device.udid,
                      label: deviceDisplayName(device, devices, true),
                      description: unavailable ? `Уже выбран как ${kind === "source" ? "новый" : "старый"} iPhone` : [deviceSubtitle(device), connection].filter(Boolean).join(" · "),
                      icon: <Smartphone size={18} />,
                      disabled: unavailable,
                    };
                  })}
                  onChange={value => { reset(); if (kind === "source") setSource(value); else setTarget(value); }}
                />
              </div>
            </div>)}
          </div>
          {sourceDevice && targetDevice && <p key={`${source}-${target}`} className="transfer-route transfer-stage" role="status"><span>{deviceDisplayName(sourceDevice, devices, true)}</span><ArrowRight size={15} aria-hidden="true" /><span>{deviceDisplayName(targetDevice, devices, true)}</span></p>}
          <p className="mt-3 text-xs leading-5 text-muted">Подключите оба iPhone и подтвердите доверие. Приложения установятся заново из доступных источников и покупок Apple ID. Данные и настройки приложений не переносятся.</p>
          {devices.length < 2 && <p className="mt-3 text-sm text-brand-300">Подключите второй iPhone, затем обновите список устройств.</p>}
          {!preview && !result && <div className="mt-4 flex flex-wrap gap-2">
            <button className="btn-primary" disabled={!!busy || !connected || source === target} onClick={() => void inspect()}>{busy ? <LoaderCircle size={15} className="animate-spin" /> : <ArrowRight size={15} />}{busy ? "Читаем приложения и проверяем доступность…" : "Выбрать приложения"}</button>
            <button className="btn-secondary" disabled={!!busy} onClick={() => void onRefresh()}><RefreshCw size={15} />Обновить устройства</button>
          </div>}
          {busy && <div role="status" className="transfer-stage mt-4 text-xs text-muted">{busy === "preview" ? "Проверка большого списка может занять несколько минут." : "Проверяем покупки Apple ID и добавляем приложения в очередь…"}</div>}
          {error && <p role="alert" className="mt-4 rounded-xl bg-red-400/10 p-3 text-sm text-red-400">{error}</p>}
          {preview && <div className="transfer-stage mt-5">
            <div className="flex flex-wrap items-center justify-between gap-3"><h3 className="text-sm font-bold">На {targetDevice ? deviceDisplayName(targetDevice, devices) : preview.target_device}</h3><span className="text-xs text-muted">Выбрано {selected.size} из {ready.length} доступных</span></div>
            <div className="my-3 flex flex-wrap items-center gap-3">
              <label className="flex min-w-48 flex-1 items-center gap-2 rounded-xl border border-white/10 px-3"><Search size={15} className="text-muted" /><input className="field min-w-0 flex-1 border-0 bg-transparent" aria-label="Поиск приложений для переноса" placeholder="Найти приложение" value={query} onChange={e => setQuery(e.target.value)} /></label>
              <button className="btn-secondary" disabled={!!busy || !ready.length} onClick={() => setSelected(new Set(selected.size === ready.length ? [] : ready))}>{selected.size === ready.length && ready.length ? "Снять выбор" : "Выбрать все"}</button>
            </div>
            <div className="max-h-80 space-y-2 overflow-y-auto pr-1">
              {preview.items.map((row, index) => ({ row, index })).filter(({ row }) => `${row.item.name} ${row.item.bundle_id ?? ""}`.toLowerCase().includes(query.toLowerCase())).map(({ row, index }) => <div key={row.item.bundle_id ?? index} className={`transfer-app flex items-center gap-3 rounded-xl border p-3 ${selected.has(index) ? "border-brand-400/30 bg-brand-400/[0.06]" : "border-white/[0.06] bg-white/[0.02]"}`}>
                <Checkbox aria-label={`Перенести ${row.item.name}`} disabled={!!busy || row.status !== "ready"} checked={selected.has(index)} onChange={checked => setSelected(current => { const next = new Set(current); if (checked) next.add(index); else next.delete(index); return next; })} />
                <div className="min-w-0 flex-1"><div className="text-sm font-semibold">{row.item.name}</div><p className="mt-1 text-xs leading-5 text-muted">{row.message}</p></div>
                <span className={`text-right text-xs ${row.status === "ready" ? "text-brand-300" : "text-muted"}`}>{statusLabels[row.status]}</span>
              </div>)}
              {!preview.items.length && <p className="p-4 text-center text-sm text-muted">На старом iPhone не найдено пользовательских приложений.</p>}
              {!!preview.items.length && !preview.items.some(row => `${row.item.name} ${row.item.bundle_id ?? ""}`.toLowerCase().includes(query.toLowerCase())) && <p className="p-4 text-center text-sm text-muted">По запросу ничего не найдено.</p>}
            </div>
            <div className="mt-4 flex flex-wrap gap-2"><button className="btn-primary" disabled={!!busy || !selected.size || !connected} onClick={() => void transfer()}>{busy ? <LoaderCircle size={15} className="animate-spin" /> : <ArrowRightLeft size={15} />}Перенести выбранные ({selected.size})</button><button className="btn-secondary" disabled={!!busy} onClick={reset}>Назад</button></div>
          </div>}
          {result && <div className="transfer-stage mt-5 rounded-2xl border border-brand-400/20 bg-brand-400/[0.05] p-4" role="status">
            <h3 className="flex items-center gap-2 font-bold"><CheckCircle2 size={19} className="text-brand-300" />{result.jobIds.length ? `В очереди: ${result.jobIds.length}` : "Приложения не добавлены"}</h3>
            {!!result.jobIds.length && <p className="mt-2 text-xs leading-5 text-muted">Установка продолжается в разделе «Установки». Оставьте новый iPhone подключённым.</p>}
            {!!result.skipped.length && <div className="mt-3 max-h-48 overflow-y-auto text-xs leading-5 text-muted"><p className="font-semibold">Пропущено: {result.skipped.length}</p>{result.skipped.map((item, index) => <p key={index}>{item.reason}</p>)}</div>}
            <div className="mt-4 flex gap-2">{!!result.jobIds.length && <Link className="btn-primary" to="/installs">Открыть установки<ArrowRight size={15} /></Link>}<button className="btn-secondary" disabled={!!busy} onClick={reset}>Новый перенос</button></div>
          </div>}
        </div>
      </div>
    </div>
  </section>;
}
