import { listen } from "@tauri-apps/api/event";
import { Check, ClipboardCopy, LoaderCircle, X } from "lucide-react";
import QRCode from "qrcode";
import { useEffect, useState } from "react";
import { command } from "../lib/tauri";
import type { InstallServerState } from "../types";

function formatBytes(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} МБ`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(0)} КБ`;
  return `${bytes} Б`;
}

/**
 * Модалка Wi-Fi сервера установки: живёт, пока Rust-сторона раздаёт IPA.
 * Показывает QR со ссылкой, прогресс скачивания на iPhone и кнопку стоп.
 */
export default function InstallServerModal() {
  const [state, setState] = useState<InstallServerState | null>(null);
  const [qr, setQr] = useState("");
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    void command<InstallServerState | null>("get_install_server_state")
      .then(s => {
        if (s && !s.stopped) setState(s);
      })
      .catch(() => undefined);
    const unlisten = listen<InstallServerState>("install-server-state", ({ payload }) => {
      setState(payload.stopped ? null : payload);
    });
    return () => {
      void unlisten.then(fn => fn());
    };
  }, []);

  useEffect(() => {
    setCopied(false);
    if (!state?.url) {
      setQr("");
            return;
    }
    let cancelled = false;
    void QRCode.toDataURL(state.url, { width: 360, margin: 1, color: { dark: "#0b1220", light: "#ffffff" } })
      .then(data => {
        if (!cancelled) setQr(data);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [state?.url]);

  if (!state) return null;

  const percent = state.size > 0 ? Math.min(100, Math.round((state.downloaded / state.size) * 100)) : 0;

  async function stopServer() {
    try {
      await command("stop_install_server");
    } catch {
      // Модалка закроется по событию stopped даже при ошибке.
    }
    setState(null);
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-6 backdrop-blur-sm">
      <div className="w-full max-w-md rounded-2xl border border-white/10 bg-[#141d30] p-6 text-center shadow-2xl">
        <div className="flex items-start justify-between gap-3">
          <div className="text-left">
          <div className="text-base font-semibold text-slate-100">Wi-Fi сервер установки</div>
          <div className="mt-1 text-xs text-slate-400">
            {state.title}
            {state.version ? ` · версия ${state.version}` : ""} · {formatBytes(state.size)}
          </div>
          </div>
          <button type="button" className="rounded-lg p-1.5 text-slate-400 transition hover:bg-white/5 hover:text-slate-200" onClick={() => void stopServer()} title="Остановить сервер">
            <X size={16} />
          </button>
        </div>

        <div className="mt-4 flex justify-center">
          {qr ? (
            <img src={qr} alt="QR со ссылкой установки" className="h-52 w-52 rounded-xl bg-white p-2" />
          ) : (
            <div className="flex h-52 w-52 items-center justify-center rounded-xl bg-white/5 text-slate-500">
              <LoaderCircle className="animate-spin" size={28} />
            </div>
          )}
        </div>

        <p className="mt-4 text-[12px] leading-5 text-slate-400">
          {state.complete
            ? "Приложение скачано — iPhone устанавливает его сам."
            : "Наведите камеру iPhone на код и откройте ссылку, затем нажмите «Установить»."}
        </p>

        <button
          type="button"
          className="mt-3 flex w-full items-center gap-2 rounded-lg border border-white/10 bg-black/20 px-3 py-2 text-left text-xs text-slate-300 transition hover:border-white/20"
          onClick={() => {
            void navigator.clipboard?.writeText(state.url).then(() => {
              setCopied(true);
              window.setTimeout(() => setCopied(false), 2000);
            });
          }}
          title="Скопировать ссылку"
        >
          <span className="truncate font-mono">{state.url}</span>
          {copied ? <Check size={14} className="ml-auto shrink-0 text-emerald-400" /> : <ClipboardCopy size={14} className="ml-auto shrink-0 text-slate-500" />}
        </button>

        <div className="mt-3 h-1.5 overflow-hidden rounded-full bg-white/10">
          <div
            className="h-full rounded-full bg-gradient-to-r from-brand-500 to-blue-400 transition-all"
            style={{ width: `${state.complete ? 100 : percent}%` }}
          />
        </div>
        <div className="mt-1.5 text-[11px] text-slate-500">
          {state.complete ? "Готово" : `Скачано ${formatBytes(state.downloaded)} из ${formatBytes(state.size)} (${percent}%)`}
        </div>

        <p className="mt-4 text-[11px] leading-4 text-slate-600">
          Не закрывайте Signum и не отключайте Wi-Fi, пока iPhone скачивает приложение.
        </p>
      </div>
    </div>
  );
}
