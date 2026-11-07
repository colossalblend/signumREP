import { listen } from "@tauri-apps/api/event";
import { AlertTriangle, Cloud, LoaderCircle, X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { command, errorMessage } from "../lib/tauri";
import type { IcloudEnsureResult, IcloudSetupProgress } from "../types";

function isWindowsClient(): boolean {
  if (typeof navigator === "undefined") return false;
  const ua = navigator.userAgent || "";
  const platform = (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData
    ?.platform;
  return /Win/i.test(platform || "") || /Windows/i.test(ua) || /Win32|Win64/i.test(navigator.platform);
}

   
                                                                                      
                                         
   
export default function IcloudSetupGate() {
  const [ready, setReady] = useState(true);
  const [bannerVisible, setBannerVisible] = useState(false);
  const [modalOpen, setModalOpen] = useState(false);
  const [phase, setPhase] = useState("checking");
  const [percent, setPercent] = useState(0);
  const [message, setMessage] = useState("Проверяем…");
  const [failed, setFailed] = useState(false);
  const [busy, setBusy] = useState(false);

  const markReady = useCallback(() => {
    setReady(true);
    setBannerVisible(false);
    setModalOpen(false);
    setFailed(false);
    setBusy(false);
  }, []);

  const markMissing = useCallback(() => {
    setReady(false);
    setBannerVisible(true);
  }, []);

  async function runEnsure(showModal: boolean) {
    if (busy) return;
    setBusy(true);
    setFailed(false);
    setPhase("checking");
    setPercent(0);
    setMessage("Проверяем…");
    if (showModal) setModalOpen(true);

    try {
      const quick = await command<IcloudEnsureResult>("check_icloud_installed");
      if (quick.installed || quick.status === "ready" || quick.status === "not_needed") {
        markReady();
        return;
      }

      markMissing();
      if (showModal) {
        setModalOpen(true);
        setPhase("missing");
        setMessage("Готовим установку…");
      }

      const result = await command<IcloudEnsureResult>("ensure_icloud_dependency", { force: false });
      if (result.installed || result.status === "ready" || result.status === "installed") {
        setPhase("ready");
        setPercent(100);
        setMessage(result.message || "Готово");
        setTimeout(() => markReady(), 1000);
        return;
      }

      markMissing();
      setFailed(true);
      setPhase("failed");
      setMessage(result.message || "Не удалось завершить установку. Повторите.");
      if (showModal) setModalOpen(true);
    } catch (error) {
      markMissing();
      setFailed(true);
      setPhase("failed");
      setMessage(errorMessage(error));
      if (showModal) setModalOpen(true);
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    if (!isWindowsClient()) {
      setReady(true);
      return;
    }

    let cancelled = false;
    const unlisten = listen<IcloudSetupProgress>("icloud-setup-progress", ({ payload }) => {
      if (cancelled) return;
      if (payload.phase === "ready") {
                                                                                    
        setFailed(false);
        setPhase("ready");
        setBusy(false);
        if (typeof payload.percent === "number") setPercent(payload.percent);
        if (payload.message) setMessage(payload.message);
        window.setTimeout(() => {
          if (!cancelled) markReady();
        }, 700);
        return;
      }
      if (
        payload.phase === "missing"
        || payload.phase === "downloading"
        || payload.phase === "installing"
        || payload.phase === "failed"
      ) {
                                                                               
                                                                            
        if (payload.phase === "failed") {
          markMissing();
          setModalOpen(true);
          setFailed(true);
        } else {
          setReady(false);
          setFailed(false);
                                                                                      
          if (payload.phase === "downloading" || payload.phase === "installing") {
            setModalOpen(true);
          } else if (payload.phase === "missing") {
            setBannerVisible(true);
          }
        }
      }
      setPhase(payload.phase);
      if (typeof payload.percent === "number") setPercent(payload.percent);
      if (payload.message) setMessage(payload.message);
    });

                                                                                        
    void runEnsure(true);

    return () => {
      cancelled = true;
      void unlisten.then(fn => fn());
    };
                                                           
  }, []);

  if (!isWindowsClient()) return null;

  const showProgress = phase === "downloading" || phase === "installing" || phase === "missing";

  return (
    <>
      {                                                  }
      {bannerVisible && !ready && (
        <div className="relative z-40 border-b border-amber-400/20 bg-amber-500/15 px-4 py-3 backdrop-blur-xl">
          <div className="flex flex-wrap items-center gap-3">
            <div className="grid h-9 w-9 shrink-0 place-items-center rounded-xl bg-amber-400/15 text-amber-300">
              <AlertTriangle size={18} />
            </div>
            <div className="min-w-0 flex-1">
              <div className="text-sm font-semibold text-amber-100">
                Для подключения iPhone нужен драйвер Apple
              </div>
              <p className="mt-0.5 text-xs leading-5 text-amber-100/80">
                Нужен драйвер Apple для iPhone — без него Windows не видит телефон по USB.
                Signum поставит только его, без iTunes.
              </p>
            </div>
            <div className="flex shrink-0 items-center gap-2">
              <button
                type="button"
                className="rounded-xl bg-amber-400 px-3.5 py-2 text-xs font-bold text-amber-950 shadow transition hover:bg-amber-300 disabled:opacity-60"
                disabled={busy}
                onClick={() => void runEnsure(true)}
              >
                {busy ? "Установка…" : "Установить"}
              </button>
            </div>
          </div>
        </div>
      )}

      {                                     }
      {modalOpen && (
        <div className="fixed inset-0 z-[80] grid place-items-center bg-black/70 p-6 backdrop-blur-md">
          <div className="panel w-full max-w-md p-6 shadow-2xl">
            <div className="flex items-start justify-between gap-3">
              <div className="flex items-center gap-3">
                <div className="grid h-11 w-11 place-items-center rounded-xl border border-sky-400/20 bg-sky-400/10 text-sky-300">
                  {busy && !failed ? (
                    <LoaderCircle size={20} className="animate-spin" />
                  ) : (
                    <Cloud size={20} />
                  )}
                </div>
                <div>
                  <h2 className="text-sm font-bold text-slate-100">Драйвер Apple</h2>
                  <p className="mt-0.5 text-[11px] text-slate-500">Нужен, чтобы видеть iPhone по USB</p>
                </div>
              </div>
              {(failed || !busy) && (
                <button
                  className="rounded-lg p-2 text-slate-500 hover:bg-white/5 hover:text-white"
                  onClick={() => {
                    setModalOpen(false);
                    if (!ready) setBannerVisible(true);
                  }}
                  aria-label="Закрыть"
                >
                  <X size={16} />
                </button>
              )}
            </div>

            <p className="mt-5 whitespace-pre-line text-sm leading-6 text-slate-300">{message}</p>

            {showProgress && !failed && (
              <div className="mt-4">
                <div className="h-1.5 overflow-hidden rounded-full bg-white/10">
                  <div
                    className="h-full rounded-full bg-sky-400 transition-all duration-300"
                    style={{ width: `${Math.max(4, Math.min(100, percent))}%` }}
                  />
                </div>
                <div className="mt-2 text-[11px] text-slate-500">
                  {phase === "installing"
                    ? "Устанавливаем… если Windows спросит разрешение — нажмите «Да»"
                    : `Загрузка ${Math.round(percent)}%`}
                </div>
              </div>
            )}

            {failed && (
              <div className="mt-5 space-y-2">
                <button
                  className="btn-primary w-full"
                  disabled={busy}
                  onClick={() => void runEnsure(true)}
                >
                  Повторить
                </button>
                {(message.includes("1603")
                  || /мультимед|перезагруз|компонент|Проигрыватель|Media/i.test(message)) && (
                  <button
                    className="btn-secondary w-full"
                    disabled={busy}
                    onClick={() => {
                      void command("open_optional_features").catch(() => undefined);
                    }}
                  >
                    Открыть параметры Windows
                  </button>
                )}
                <button
                  className="btn-secondary w-full"
                  disabled={busy}
                  onClick={() => {
                    setModalOpen(false);
                    setBannerVisible(true);
                  }}
                >
                  Позже
                </button>
              </div>
            )}
          </div>
        </div>
      )}
    </>
  );
}
