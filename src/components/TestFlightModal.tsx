import { CheckCircle2, LoaderCircle, Smartphone, X } from "lucide-react";
import { useEffect, useState } from "react";
import QRCode from "qrcode";
import { command, errorMessage } from "../lib/tauri";
import type { CatalogApp, InstalledApp } from "../types";

/** Apple's own TestFlight app — a free App Store title like any other. */
const TESTFLIGHT_APP_STORE_ID = "899247664";
const TESTFLIGHT_BUNDLE_ID = "com.apple.TestFlight";

type Step = "checking" | "needs-testflight" | "installing" | "ready";

export default function TestFlightModal({
  app,
  inviteUrl,
  onClose,
}: {
  app: CatalogApp;
  inviteUrl: string;
  onClose: () => void;
}) {
  const [step, setStep] = useState<Step>("checking");
  const [qr, setQr] = useState("");
  const [error, setError] = useState("");

  // The invite never changes while the card is open, so render it once.
  useEffect(() => {
    let cancelled = false;
    QRCode.toDataURL(inviteUrl, { width: 320, margin: 1 })
      .then(url => { if (!cancelled) setQr(url); })
      .catch(() => { if (!cancelled) setQr(""); });
    return () => { cancelled = true; };
  }, [inviteUrl]);

  // Step 1: is TestFlight already on the iPhone? If it is, skip straight to the
  // invite — installing it again would only waste the user's time.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const installed = await command<InstalledApp[]>("list_installed_apps");
        if (cancelled) return;
        const present = installed.some(item => item.bundle_id === TESTFLIGHT_BUNDLE_ID);
        setStep(present ? "ready" : "needs-testflight");
      } catch {
        // No device, or it is locked. Show the invite anyway — the QR is
        // scanned with the phone's camera and does not need a USB connection.
        if (!cancelled) setStep("ready");
      }
    })();
    return () => { cancelled = true; };
  }, []);

  async function installTestFlight() {
    setStep("installing");
    setError("");
    try {
      await command<string>("enqueue_catalog_install", {
        input: {
          title: "TestFlight",
          bundle_id: TESTFLIGHT_BUNDLE_ID,
          app_store_id: TESTFLIGHT_APP_STORE_ID,
          in_store: true,
          already_owned: false,
          min_ios: null,
          version: null,
        },
      });
      setStep("ready");
    } catch (e) {
      setError(errorMessage(e));
      setStep("needs-testflight");
    }
  }

  const steps = [
    "Откройте камеру на iPhone и наведите её на этот код.",
    "Нажмите на всплывшую ссылку — откроется TestFlight.",
    "В TestFlight нажмите «Принять», затем «Установить».",
    `Готово — ${app.title} появится на экране «Домой».`,
  ];

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4">
      <div className="w-full max-w-md rounded-2xl border border-white/[0.08] bg-[#12131a] p-5">
        <div className="flex items-start gap-3">
          {app.icon_url && (
            <img src={app.icon_url} alt="" className="size-12 rounded-xl" />
          )}
          <div className="min-w-0 flex-1">
            <div className="truncate text-base font-semibold text-white">{app.title}</div>
            <div className="text-xs text-slate-400">Установка через TestFlight</div>
          </div>
          <button type="button" onClick={onClose} className="text-slate-400 hover:text-white">
            <X size={18} />
          </button>
        </div>

        {step === "checking" && (
          <p className="mt-5 flex items-center gap-2 text-xs text-slate-400">
            <LoaderCircle size={13} className="animate-spin text-brand-300" />
            Проверяем, установлен ли TestFlight на iPhone…
          </p>
        )}

        {step === "needs-testflight" && (
          <div className="mt-5 rounded-xl border border-amber-400/25 bg-amber-400/10 px-3 py-3">
            <div className="text-[11px] font-semibold text-amber-200">
              На iPhone нет TestFlight
            </div>
            <p className="mt-1 text-[11px] leading-4 text-amber-100/80">
              Это бесплатное приложение Apple. Signum установит его сам — или поставьте
              из App Store вручную.
            </p>
            <button
              type="button"
              className="btn-primary mt-3 w-full"
              onClick={() => void installTestFlight()}
            >
              <Smartphone size={15} />
              Установить TestFlight
            </button>
          </div>
        )}

        {step === "installing" && (
          <p className="mt-5 flex items-center gap-2 text-xs text-slate-400">
            <LoaderCircle size={13} className="animate-spin text-brand-300" />
            Ставим TestFlight на iPhone…
          </p>
        )}

        {step === "ready" && (
          <>
            <div className="mt-5 flex justify-center rounded-xl bg-white p-3">
              {qr ? (
                <img src={qr} alt="QR-код приглашения TestFlight" className="size-56" />
              ) : (
                <div className="flex size-56 items-center justify-center text-xs text-slate-500">
                  Не удалось построить код
                </div>
              )}
            </div>

            <ol className="mt-4 space-y-2">
              {steps.map((text, index) => (
                <li key={index} className="flex gap-2.5 text-[11px] leading-4 text-slate-300">
                  <span className="flex size-4 shrink-0 items-center justify-center rounded-full bg-brand-500/20 text-[10px] font-semibold text-brand-200">
                    {index + 1}
                  </span>
                  {text}
                </li>
              ))}
            </ol>

            <p className="mt-3 flex items-start gap-1.5 px-1 text-[11px] leading-4 text-slate-500">
              <CheckCircle2 size={12} className="mt-0.5 shrink-0 text-emerald-400" />
              Apple ID для этого не нужен — приглашение открывается прямо на телефоне.
            </p>

            <div className="mt-2 break-all rounded-lg bg-black/30 px-2.5 py-2 text-[10px] text-slate-500">
              {inviteUrl}
            </div>
          </>
        )}

        {error && <p className="mt-3 text-[11px] leading-4 text-rose-300">{error}</p>}

        <button type="button" className="btn-secondary mt-4 w-full" onClick={onClose}>
          Закрыть
        </button>
      </div>
    </div>
  );
}
