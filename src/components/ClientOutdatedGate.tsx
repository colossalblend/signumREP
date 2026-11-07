import { listen } from "@tauri-apps/api/event";
import { AlertTriangle, Download, LoaderCircle } from "lucide-react";
import { useEffect, useState } from "react";
import { command, errorMessage } from "../lib/tauri";
import type { ClientOutdatedInfo, OtaUpdateInfo } from "../types";

   
                                                                         
                                                                        
   
export default function ClientOutdatedGate() {
  const [info, setInfo] = useState<ClientOutdatedInfo | null>(null);
  const [checking, setChecking] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    let cancelled = false;

    void (async () => {
      try {
        const ota = await command<OtaUpdateInfo>("check_for_update");
        if (cancelled) return;
        if (ota.client_supported === false) {
          setInfo({
            message:
              ota.notes?.trim()
              || "Приложение недоступно. Эта версия Signum устарела.",
            min_client_version: ota.min_client_version,
            latest_version: ota.version,
            download_url: ota.download_url,
            sha256: ota.sha256,
          });
        }
      } catch {
                                                     
      } finally {
        if (!cancelled) setChecking(false);
      }
    })();

    const unlisten = listen<ClientOutdatedInfo>("client-outdated", ({ payload }) => {
      setInfo({
        message: payload.message || "Приложение недоступно. Обновите Signum.",
        min_client_version: payload.min_client_version,
        latest_version: payload.latest_version,
        download_url: payload.download_url,
        sha256: payload.sha256,
      });
      setChecking(false);
    });

    return () => {
      cancelled = true;
      void unlisten.then(fn => fn());
    };
  }, []);

  async function installUpdate() {
    if (!info?.download_url || busy) return;
    setBusy(true);
    setError("");
    try {
      const path = await command<string>("download_update", {
        url: info.download_url,
        expectedSha256: info.sha256,
      });
      await command<void>("install_downloaded_update", { path });
    } catch (e) {
      setError(errorMessage(e));
      setBusy(false);
    }
  }

  if (checking || !info) return null;

  return (
    <div className="fixed inset-0 z-[200] flex items-center justify-center bg-[#070912]/95 p-6 backdrop-blur-md">
      <div className="panel w-full max-w-md p-8 text-center shadow-panel">
        <div className="mx-auto grid h-14 w-14 place-items-center rounded-2xl border border-rose-400/25 bg-rose-400/10 text-rose-300">
          <AlertTriangle size={28} />
        </div>
        <h1 className="mt-5 text-xl font-bold text-slate-50">Приложение недоступно</h1>
        <p className="mt-3 text-sm leading-6 text-slate-400">
          {info.message}
        </p>
        {(info.min_client_version || info.latest_version) && (
          <p className="mt-3 text-xs text-slate-500">
            {info.min_client_version
              ? `Нужна версия ${info.min_client_version} или новее`
              : null}
            {info.min_client_version && info.latest_version ? " · " : null}
            {info.latest_version ? `Актуальная: ${info.latest_version}` : null}
          </p>
        )}
        <p className="mt-4 text-xs leading-5 text-slate-600">
          Управление приложением заблокировано, пока Signum не обновлён.
        </p>
        {error && (
          <p className="mt-4 rounded-xl border border-rose-400/20 bg-rose-400/10 px-3 py-2 text-xs text-rose-200">
            {error}
          </p>
        )}
        {info.download_url ? (
          <button
            type="button"
            className="btn-primary mt-6 w-full"
            disabled={busy}
            onClick={() => void installUpdate()}
          >
            {busy ? (
              <LoaderCircle size={16} className="animate-spin" />
            ) : (
              <Download size={16} />
            )}
            {busy ? "Обновление…" : "Обновить Signum"}
          </button>
        ) : (
          <p className="mt-6 text-xs text-slate-500">
            Ссылка на обновление недоступна. Скачайте новую версию с сайта Signum.
          </p>
        )}
      </div>
    </div>
  );
}
