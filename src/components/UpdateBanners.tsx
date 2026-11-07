import { listen } from "@tauri-apps/api/event";
import { LoaderCircle, LogIn, Sparkles, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { command, errorMessage } from "../lib/tauri";
import type { AppUpdateFoundEvent, OtaUpdateInfo } from "../types";

type AppBanner = AppUpdateFoundEvent;
type OtaBanner = { update: OtaUpdateInfo; auto_install?: boolean };

export default function UpdateBanners() {
  const [appUpdate, setAppUpdate] = useState<AppBanner | null>(null);
  const [otaUpdate, setOtaUpdate] = useState<OtaBanner | null>(null);
  const [otaBusy, setOtaBusy] = useState(false);
  const [otaPercent, setOtaPercent] = useState(0);
  const [otaError, setOtaError] = useState<string | null>(null);
  const [appUpdateError, setAppUpdateError] = useState<string | null>(null);
  const otaInFlight = useRef(false);
  const [legacyLogout, setLegacyLogout] = useState(false);
  const [sessionExpired, setSessionExpired] = useState(false);
  const navigate = useNavigate();

  useEffect(() => {
    const unsubs = [
      listen<AppBanner>("app-update-found", ({ payload }) => setAppUpdate(payload)),
      listen<{ update: OtaUpdateInfo; auto_install?: boolean }>("ota-available", ({ payload }) => {
        setOtaUpdate(payload);
        if (payload.auto_install && payload.update.download_url) {
          void runOtaInstall(payload.update);
        }
      }),
      listen<{ percent: number }>("ota-progress", ({ payload }) => setOtaPercent(payload.percent)),
      // Срок входа истекает посреди работы, а не при запуске: ловим событием,
      // иначе человек узнал бы об этом только после перезапуска.
      listen("session-expired", () => setSessionExpired(true)),
    ];
    return () => { void Promise.all(unsubs).then(handles => handles.forEach(unlisten => unlisten())); };
  }, []);

  useEffect(() => {
    // Вход, записанный снятыми движками v2/v3, при запуске снимается — здесь
    // об этом узнают один раз и говорят человеку, почему он вышел.
    void command<boolean>("take_legacy_logout_notice").then(setLegacyLogout).catch(() => {});
    void command<boolean>("take_session_expired_notice").then(setSessionExpired).catch(() => {});
  }, []);

  async function installAppUpdate() {
    if (!appUpdate) return;
    setAppUpdateError(null);
    try {
      await command<string>("enqueue_catalog_install", {
        input: {
          title: appUpdate.title,
          bundle_id: appUpdate.bundle_id,
          app_store_id: appUpdate.app_store_id,
          in_store: appUpdate.in_store,
          min_ios: appUpdate.min_ios,
        },
      });
    } catch (error) {
      // Без этого кнопка «Обновить» молча ничего не делала: нет iPhone, нет
      // входа — человек жал ещё раз и не понимал, что происходит.
      setAppUpdateError(errorMessage(error));
      return;
    }
    setAppUpdate(null);
    navigate("/installs");
  }

  async function runOtaInstall(update: OtaUpdateInfo) {
    if (!update.download_url || otaInFlight.current) return;
    otaInFlight.current = true;
    setOtaError(null);
    setOtaBusy(true);
    setOtaPercent(0);
    try {
      const path = await command<string>("download_update", {
        url: update.download_url,
        expectedSha256: update.sha256,
      });
      await command<void>("install_downloaded_update", { path });
    } catch (error) {
      setOtaError(errorMessage(error));
    } finally {
      otaInFlight.current = false;
      setOtaBusy(false);
    }
  }

  if (!appUpdate && !otaUpdate && !legacyLogout && !sessionExpired) return null;

  return (
    <div className="relative z-40 space-y-2 border-b border-white/[0.06] bg-[#0a1020]/95 px-4 py-3 backdrop-blur-xl">
      {sessionExpired && (
        <Banner
          icon={<LogIn size={16} />}
          title="Срок входа в Apple ID истёк"
          text="Apple перестала принимать сохранённый вход. Войдите заново — список версий и загрузки сразу заработают."
          actionLabel="Войти"
          onAction={() => { setSessionExpired(false); navigate("/account"); }}
          onDismiss={() => setSessionExpired(false)}
        />
      )}
      {legacyLogout && (
        <Banner
          icon={<LogIn size={16} />}
          title="Нужен повторный вход в Apple ID"
          text="После обновления нужно снова войти в Apple ID. Это потребуется только один раз."
          actionLabel="Войти"
          onAction={() => { setLegacyLogout(false); navigate("/account"); }}
          onDismiss={() => setLegacyLogout(false)}
        />
      )}
      {appUpdate && (
        <Banner
          icon={<Sparkles size={16} />}
          title={`Доступно обновление ${appUpdate.title}`}
          text={appUpdateError ?? `${appUpdate.installed_version} → ${appUpdate.remote_version}`}
          actionLabel={appUpdateError ? "Повторить" : "Обновить"}
          onAction={() => void installAppUpdate()}
          onDismiss={() => { setAppUpdate(null); setAppUpdateError(null); }}
        />
      )}
      {otaUpdate && (
        <Banner
          icon={otaBusy ? <LoaderCircle size={16} className="animate-spin" /> : <Sparkles size={16} />}
          title={`Доступна новая версия Signum ${otaUpdate.update.version ?? ""}`}
          text={otaBusy ? `Загрузка обновления… ${Math.round(otaPercent)}%` : (otaError ?? otaUpdate.update.notes ?? "Рекомендуем обновить Signum до последней версии.")}
          actionLabel={otaBusy ? "Загрузка…" : otaError ? "Повторить загрузку" : "Обновить Signum"}
          actionDisabled={otaBusy || !otaUpdate.update.download_url}
          onAction={() => void runOtaInstall(otaUpdate.update)}
          onDismiss={otaUpdate.update.mandatory || otaBusy ? undefined : () => setOtaUpdate(null)}
        />
      )}
    </div>
  );
}

function Banner({ icon, title, text, actionLabel, onAction, onDismiss, actionDisabled = false }: {
  icon: React.ReactNode; title: string; text: string; actionLabel: string;
  onAction: () => void; onDismiss?: () => void; actionDisabled?: boolean;
}) {
  return (
    <div className="flex items-center justify-between gap-4 rounded-xl border border-brand-400/20 bg-brand-500/10 px-4 py-3">
      <div className="flex min-w-0 items-start gap-3">
        <div className="mt-0.5 text-brand-300">{icon}</div>
        <div className="min-w-0">
          <div className="text-sm font-semibold text-white">{title}</div>
          <div className="mt-0.5 truncate text-xs text-slate-400">{text}</div>
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-2">
        <button disabled={actionDisabled} onClick={onAction} className="btn-primary px-3 py-2 text-xs">{actionLabel}</button>
        {onDismiss && (
          <button onClick={onDismiss} className="rounded-lg p-2 text-slate-500 transition hover:bg-white/5 hover:text-white" aria-label="Скрыть">
            <X size={15} />
          </button>
        )}
      </div>
    </div>
  );
}
