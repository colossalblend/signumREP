import { Copy, LoaderCircle, QrCode, RefreshCw, Smartphone, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  activationPayload,
  hydrateActivation,
  type HydratedActivationItem,
} from "../lib/activation";
import type { AppActivation, AppActivationItem } from "../types";

function qrImageUrl(data: string, size = 200): string {
  return `https://api.qrserver.com/v1/create-qr-code/?size=${size}x${size}&margin=8&data=${encodeURIComponent(data)}`;
}

function formatTtl(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const m = Math.floor(s / 60);
  const r = s % 60;
  return `${m}:${r.toString().padStart(2, "0")}`;
}

function modeHint(item: AppActivationItem): string {
  if (item.hint?.trim()) return item.hint.trim();
  if (item.mode === "in_app_scan") {
    return "Откройте приложение и отсканируйте QR внутри него (не через системную камеру iPhone).";
  }
  return "Отсканируйте QR камерой iPhone — откроется приложение. QR действует 10 минут.";
}

function QrBlock({
  item,
  now,
}: {
  item: HydratedActivationItem;
  now: number;
}) {
  const payload = activationPayload(item);
  const [copied, setCopied] = useState(false);
  const [imgFailed, setImgFailed] = useState(false);
  const inApp = item.mode === "in_app_scan";
  const missing = !payload;
  const ttlLeft =
    item.expires_at != null ? Math.max(0, Math.floor((item.expires_at - now) / 1000)) : null;
  const expired = ttlLeft != null && ttlLeft <= 0;

  async function copyPayload() {
    if (!payload || expired) return;
    try {
      await navigator.clipboard.writeText(payload);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch {
      setCopied(false);
    }
  }

  return (
    <div className="rounded-2xl border border-white/[0.08] bg-black/20 p-4">
      <div className="mb-3 flex items-start justify-between gap-3">
        <div>
          <div className="text-sm font-semibold text-slate-100">{item.title}</div>
          {item.subtitle?.trim() && (
            <div className="mt-0.5 text-[11px] text-slate-500">{item.subtitle}</div>
          )}
        </div>
        <div className="flex shrink-0 flex-col items-end gap-1">
          <span
            className={`rounded-lg px-2 py-1 text-[10px] font-bold uppercase ${
              inApp
                ? "bg-violet-400/10 text-violet-300"
                : "bg-sky-400/10 text-sky-300"
            }`}
          >
            {inApp ? "В приложении" : "Камера iPhone"}
          </span>
          {ttlLeft != null && (
            <span
              className={`rounded-lg px-2 py-1 text-[10px] font-bold tabular-nums ${
                expired
                  ? "bg-rose-400/15 text-rose-300"
                  : ttlLeft < 60
                    ? "bg-amber-400/15 text-amber-300"
                    : "bg-white/5 text-slate-400"
              }`}
            >
              {expired ? "истёк" : `ещё ${formatTtl(ttlLeft)}`}
            </span>
          )}
        </div>
      </div>

      <p className="mb-3 flex items-start gap-2 text-[11px] leading-5 text-slate-400">
        <Smartphone size={14} className="mt-0.5 shrink-0 text-brand-300" />
        {expired
          ? "Срок QR истёк — нажмите «Обновить QR», будет новый код."
          : modeHint(item)}
      </p>

      <div className="flex flex-col items-center gap-3 sm:flex-row sm:items-start">
        <div className={`grid h-[200px] w-[200px] shrink-0 place-items-center rounded-xl bg-white p-2 ${expired ? "opacity-40" : ""}`}>
          {!missing && !imgFailed && !expired ? (
            <img
              key={payload}
              src={qrImageUrl(payload)}
              alt={`QR ${item.title}`}
              className="h-full w-full object-contain"
              onError={() => setImgFailed(true)}
            />
          ) : (
            <div className="px-3 text-center text-[11px] leading-4 text-slate-600">
              {expired
                ? "QR устарел"
                : missing
                  ? "QR ещё не сгенерирован."
                  : "Не удалось загрузить QR. Скопируйте данные вручную."}
            </div>
          )}
        </div>
        <div className="min-w-0 flex-1 space-y-2">
          <p className="break-all rounded-lg bg-black/30 px-3 py-2 font-mono text-[10px] leading-4 text-slate-400">
            {expired ? "—" : payload || "—"}
          </p>
          <button
            type="button"
            className="btn-secondary w-full"
            disabled={!payload || expired}
            onClick={() => void copyPayload()}
          >
            <Copy size={14} />
            {copied ? "Скопировано" : inApp ? "Скопировать данные QR" : "Скопировать ссылку"}
          </button>
        </div>
      </div>
    </div>
  );
}

export default function ActivationModal({
  appTitle,
  activation,
  onClose,
}: {
  appTitle: string;
  activation: AppActivation;
  onClose: () => void;
}) {
  const [live, setLive] = useState<AppActivation | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [now, setNow] = useState(() => Date.now());

  const load = useCallback(async (force = false) => {
    setLoading(true);
    setError("");
    try {
                                                                       
      setLive(await hydrateActivation(activation, force));
      setNow(Date.now());
    } catch (e) {
      setError(e instanceof Error ? e.message : "Не удалось загрузить активацию");
      setLive(activation);
    } finally {
      setLoading(false);
    }
  }, [activation]);

  useEffect(() => {
                                                                      
    void load(false);
  }, [load]);

                                                                         
  useEffect(() => {
    const tick = window.setInterval(() => {
      setNow(Date.now());
    }, 1000);
    return () => window.clearInterval(tick);
  }, []);

  useEffect(() => {
    if (loading || !live) return;
    const items = live.items as HydratedActivationItem[];
    const anyExpired = items.some(
      i => i.tb_id && i.expires_at != null && i.expires_at <= Date.now(),
    );
    if (anyExpired) {
                                                                          
      void load(true);
    }
  }, [now, live, loading, load]);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const data = live || activation;
  const title = data.title?.trim() || "Активация после установки";
  const message =
    data.message?.trim() ||
    "Для входа нужно активировать приложение. Отсканируйте QR камерой iPhone.";
  const single = data.items.length === 1;
  const needsRefresh = data.items.some(i => !!(i.tb_id || "").trim());
  const minTtl = useMemo(() => {
    const items = (live?.items ?? []) as HydratedActivationItem[];
    const times = items
      .map(i => (i.expires_at != null ? Math.floor((i.expires_at - now) / 1000) : null))
      .filter((v): v is number => v != null);
    if (!times.length) return null;
    return Math.max(0, Math.min(...times));
  }, [live, now]);

  return (
    <div
      className="fixed inset-0 z-[70] grid place-items-center bg-black/75 p-6 backdrop-blur-md"
      onMouseDown={onClose}
    >
      <div
        className="panel flex max-h-[90vh] w-full max-w-2xl flex-col p-6"
        onMouseDown={e => e.stopPropagation()}
      >
        <div className="flex items-start justify-between gap-4">
          <div className="flex items-start gap-3">
            <div className="mt-0.5 rounded-xl bg-brand-500/15 p-2.5 text-brand-300">
              <QrCode size={20} />
            </div>
            <div>
              <h2 className="text-xl font-bold">{title}</h2>
              <p className="mt-1 text-xs text-slate-500">
                {appTitle}
                {single ? ` · ${data.items[0]?.title}` : ` · ${data.items.length} QR`}
                {data.provider === "tb" ? " · код на 10 минут" : ""}
              </p>
            </div>
          </div>
          <button
            type="button"
            className="rounded-lg p-2 text-slate-500 hover:bg-white/5 hover:text-white"
            onClick={onClose}
            aria-label="Закрыть"
          >
            <X size={18} />
          </button>
        </div>

        <p className="mt-4 text-sm leading-6 text-slate-300">{message}</p>
        {minTtl != null && !loading && (
          <p className="mt-2 text-[11px] text-slate-500">
            Следующая авто-смена QR через {formatTtl(minTtl)}
          </p>
        )}
        {error && <p className="mt-2 text-xs text-rose-300">{error}</p>}

        {loading ? (
          <div className="mt-8 flex flex-col items-center justify-center gap-3 py-12">
            <LoaderCircle className="animate-spin text-brand-300" size={28} />
            <p className="text-sm text-slate-400">Готовим QR-код…</p>
          </div>
        ) : (
          <div className="mt-5 min-h-0 flex-1 space-y-3 overflow-y-auto pr-1">
            {data.items.map(item => (
              <QrBlock key={item.id} item={item as HydratedActivationItem} now={now} />
            ))}
          </div>
        )}

        <div className="mt-5 flex flex-col gap-2 sm:flex-row">
          {needsRefresh && (
            <button
              type="button"
              className="btn-secondary w-full"
              disabled={loading}
              onClick={() => void load(true)}
            >
              <RefreshCw size={14} className={loading ? "animate-spin" : ""} />
              Обновить QR
            </button>
          )}
          <button type="button" className="btn-secondary w-full" onClick={onClose}>
            Закрыть
          </button>
        </div>
      </div>
    </div>
  );
}
