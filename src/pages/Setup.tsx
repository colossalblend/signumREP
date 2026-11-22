import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AlertTriangle, Check, FolderOpen, LayoutGrid, Monitor, RotateCcw, Rocket, X } from "lucide-react";
import { useEffect, useState } from "react";
import signumIcon from "../../assets/signum-icon.png";
import DragRegion from "../components/DragRegion";

type Page = "welcome" | "install" | "done" | "error";

type SetupInfo = {
  version: string;
  target: string;
};

type Progress = {
  percent: number;
  message: string;
};

const STAGES = [
  { from: 0, label: "Остановка прежней версии" },
  { from: 18, label: "Копирование файлов" },
  { from: 88, label: "Ярлыки и регистрация" },
];

function stageState(index: number, percent: number): "done" | "active" | "pending" {
  const next = STAGES[index + 1]?.from ?? 101;
  if (percent >= next) return "done";
  if (percent >= STAGES[index].from) return "active";
  return "pending";
}

const RING = 2 * Math.PI * 54;

function IconRing({ percent, state }: { percent: number | null; state: "idle" | "busy" | "done" | "error" }) {
  const value = percent === null ? 0 : Math.max(0, Math.min(100, percent));
  return (
    <div className={`setup-ring is-${state}`}>
      <svg viewBox="0 0 120 120" aria-hidden>
        <circle className="setup-ring-track" cx="60" cy="60" r="54" />
        <circle className="setup-ring-value" cx="60" cy="60" r="54" strokeDasharray={RING} strokeDashoffset={RING * (1 - value / 100)} />
      </svg>
      <img src={signumIcon} alt="" draggable={false} className="setup-ring-icon" />
      {state === "done" && <span className="setup-badge is-ok" aria-hidden><Check size={15} strokeWidth={3} /></span>}
      {state === "error" && <span className="setup-badge is-error" aria-hidden><X size={15} strokeWidth={3} /></span>}
    </div>
  );
}

export default function SetupApp() {
  const win = getCurrentWindow();
  const [info, setInfo] = useState<SetupInfo | null>(null);
  const [desktop, setDesktop] = useState(true);
  const [page, setPage] = useState<Page>("welcome");
  const [progress, setProgress] = useState<Progress>({ percent: 0, message: "" });
  const [error, setError] = useState("");

  useEffect(() => {
    void invoke<SetupInfo>("setup_info").then(setInfo).catch(() => undefined);
    const un = listen<Progress>("setup-progress", (event) => {
      setProgress(event.payload);
    });
    return () => {
      void un.then((fn) => fn());
    };
  }, []);

  async function install() {
    setPage("install");
    setProgress({ percent: 2, message: "Подготовка…" });
    setError("");
    try {
      await invoke("start_install", { desktopShortcut: desktop });
      setProgress({ percent: 100, message: "Готово" });
      setPage("done");
    } catch (e) {
      setError(typeof e === "string" ? e : "Не удалось установить Signum.");
      setPage("error");
    }
  }

  async function launch() {
    try {
      await invoke("launch_app");
    } catch {
                                   
    }
    await win.close();
  }

  const busy = page === "install";
  const percent = Math.round(Math.max(0, Math.min(100, progress.percent)));
  const ringState = page === "install" ? "busy" : page === "done" ? "done" : page === "error" ? "error" : "idle";

  return (
    <div className="app-shell setup-shell flex h-full flex-col">
      <div className="setup-backdrop" aria-hidden />
      <header className="relative z-30 flex h-11 shrink-0 items-stretch">
        <DragRegion className="flex h-11 min-w-0 flex-1 cursor-grab items-center gap-2 px-4 active:cursor-grabbing">
          <img src={signumIcon} alt="" draggable={false} className="pointer-events-none h-4 w-4 rounded" />
          <span className="pointer-events-none text-xs font-medium text-muted">Установка Signum</span>
        </DragRegion>
        <button
          type="button"
          aria-label="Закрыть"
          disabled={busy}
          data-no-drag
          onClick={() => void win.close()}
          className="grid h-11 w-12 place-items-center text-slate-400 transition hover:bg-red-500/80 hover:text-white disabled:pointer-events-none disabled:opacity-30"
        >
          <X size={17} />
        </button>
      </header>

      <main className="relative z-10 flex min-h-0 flex-1 flex-col px-7 pb-6">
        <div className="setup-enter flex flex-col items-center pt-2 text-center">
          <IconRing percent={page === "welcome" ? null : page === "error" ? progress.percent : percent} state={ringState} />
          <div className="mt-4 text-[22px] font-bold tracking-tight text-theme">Signum</div>
          <span className="setup-version mt-1.5">{info?.version ? `Версия ${info.version}` : "Установщик"}</span>
        </div>

        {page === "welcome" && (
          <div key="welcome" className="setup-page mt-6 flex min-h-0 flex-1 flex-col">
            <div className="setup-card">
              <div className="setup-row">
                <span className="setup-row-icon"><FolderOpen size={16} /></span>
                <div className="min-w-0 flex-1"><div className="text-xs font-semibold text-theme">Папка установки</div><div className="truncate text-[11px] text-muted" title={info?.target}>{info?.target ?? "%LOCALAPPDATA%\\Signum"}</div></div>
              </div>
              <div className="setup-row">
                <span className="setup-row-icon"><LayoutGrid size={16} /></span>
                <div className="min-w-0 flex-1"><div className="text-xs font-semibold text-theme">Ярлык в меню «Пуск»</div><div className="text-[11px] text-muted">Создаётся всегда</div></div>
                <Check size={16} className="shrink-0 text-emerald-400" />
              </div>
              <button type="button" role="checkbox" aria-checked={desktop} className="setup-row" onClick={() => setDesktop(v => !v)}>
                <span className="setup-row-icon"><Monitor size={16} /></span>
                <div className="min-w-0 flex-1"><div className="text-xs font-semibold text-theme">Ярлык на рабочем столе</div><div className="text-[11px] text-muted">{desktop ? "Будет создан" : "Не создавать"}</div></div>
                <span className={`signum-check shrink-0 ${desktop ? "is-on" : ""}`} aria-hidden><span className="signum-check-box"><svg className="signum-check-mark" viewBox="0 0 16 16" fill="none"><path d="M3.5 8.2 6.6 11.3 12.5 4.7" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" /></svg></span></span>
              </button>
            </div>
            <p className="mt-3 mb-4 text-center text-[11px] text-muted">Права администратора не нужны</p>
            <button type="button" className="btn-primary setup-cta mt-auto w-full" onClick={() => void install()}>
              Установить
            </button>
          </div>
        )}

        {page === "install" && (
          <div key="install" className="setup-page mt-5 flex flex-1 flex-col">
            <div className="flex items-baseline justify-center gap-1"><span className="text-3xl font-bold tabular-nums tracking-tight text-theme">{percent}</span><span className="text-sm font-semibold text-muted">%</span></div>
            <p key={progress.message} className="setup-message mt-1 truncate text-center text-xs text-muted">{progress.message || "Подготовка…"}</p>
            <ol className="setup-card mt-5">
              {STAGES.map((stage, i) => { const state = stageState(i, progress.percent); return <li key={stage.label} className={`setup-stage is-${state}`}><span className="setup-stage-dot">{state === "done" ? <Check size={12} strokeWidth={3} /> : state === "active" ? <span className="setup-stage-pulse" /> : null}</span>{stage.label}</li>; })}
            </ol>
            <p className="mt-auto text-center text-[11px] text-muted">Не закрывайте окно до конца установки</p>
          </div>
        )}

        {page === "done" && (
          <div key="done" className="setup-page mt-5 flex flex-1 flex-col">
            <p className="text-center text-lg font-bold text-theme">Signum установлен</p>
            <p className="mt-1 text-center text-sm text-muted">{desktop ? "Ярлыки — в меню «Пуск» и на рабочем столе." : "Ярлык — в меню «Пуск»."}</p>
            <div className="mt-auto space-y-2">
              <button type="button" className="btn-primary setup-cta w-full" onClick={() => void launch()}><Rocket size={16} />Запустить Signum</button>
              <button type="button" className="btn-secondary w-full" onClick={() => void win.close()}>Закрыть</button>
            </div>
          </div>
        )}

        {page === "error" && (
          <div key="error" className="setup-page mt-5 flex min-h-0 flex-1 flex-col">
            <p className="text-center text-lg font-bold text-theme">Не удалось установить</p>
            <div className="setup-error mt-4"><AlertTriangle size={16} className="mt-0.5 shrink-0" /><p className="min-w-0 flex-1 select-text break-words">{error}</p></div>
            <p className="mt-3 text-center text-[11px] text-muted">Закройте Signum, если он открыт, и повторите.</p>
            <div className="mt-auto space-y-2">
              <button type="button" className="btn-primary setup-cta w-full" onClick={() => void install()}><RotateCcw size={16} />Повторить</button>
              <button type="button" className="btn-secondary w-full" onClick={() => void win.close()}>Закрыть</button>
            </div>
          </div>
        )}
      </main>
    </div>
  );
}
