import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { CheckCircle2, LoaderCircle, X } from "lucide-react";
import { useEffect, useState } from "react";
import signumIcon from "../../assets/signum-icon.png";
import Checkbox from "../components/Checkbox";
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

  return (
    <div className="app-shell flex h-full flex-col">
      <header className="app-header relative z-30 flex h-11 shrink-0 items-stretch">
        <DragRegion className="flex h-11 min-w-0 flex-1 cursor-grab items-center px-4 active:cursor-grabbing">
          <span className="pointer-events-none text-xs font-medium text-muted">Установка</span>
        </DragRegion>
        <button
          type="button"
          aria-label="Закрыть"
          disabled={busy}
          data-no-drag
          onClick={() => void win.close()}
          className="grid h-11 w-12 place-items-center text-slate-400 transition hover:bg-red-500/80 hover:text-white disabled:opacity-30"
        >
          <X size={17} />
        </button>
      </header>

      <main className="flex min-h-0 flex-1 flex-col px-7 pb-6 pt-5">
        <div className="flex flex-col items-center text-center">
          <img
            src={signumIcon}
            alt=""
            draggable={false}
            className="h-16 w-16 rounded-2xl object-cover shadow-glow"
          />
          <div className="mt-4 text-xl font-bold tracking-tight text-theme">Signum</div>
          <div className="mt-1 text-xs font-medium text-muted">Версия {info?.version ?? "1.1.2"}</div>
        </div>

        {page === "welcome" && (
          <div className="mt-6 flex min-h-0 flex-1 flex-col">
            <p className="text-center text-sm text-theme">Установка на этот компьютер</p>
            {info?.target && (
              <p className="mt-2 break-all text-center text-[11px] leading-4 text-muted">{info.target}</p>
            )}
            <div className="panel mt-5 px-4 py-3">
              <Checkbox
                checked={desktop}
                onChange={setDesktop}
                label="Ярлык на рабочем столе"
              />
            </div>
            <button type="button" className="btn-primary mt-auto w-full" onClick={() => void install()}>
              Установить
            </button>
          </div>
        )}

        {page === "install" && (
          <div className="mt-8 flex flex-1 flex-col">
            <p className="text-center text-sm font-semibold text-theme">Устанавливаем Signum</p>
            <p className="mt-2 text-center text-xs text-muted">{progress.message || "Подготовка…"}</p>
            <div className="mt-6 h-2 overflow-hidden rounded-full bg-white/[0.06]">
              <div
                className="h-full rounded-full bg-brand-gradient transition-[width] duration-200"
                style={{ width: `${Math.max(4, Math.min(100, progress.percent))}%` }}
              />
            </div>
            <p className="mt-3 text-center text-sm font-semibold text-theme">{Math.round(progress.percent)}%</p>
            <div className="mt-auto flex justify-center text-muted">
              <LoaderCircle size={18} className="animate-spin" />
            </div>
          </div>
        )}

        {page === "done" && (
          <div className="mt-8 flex flex-1 flex-col">
            <div className="flex justify-center text-emerald-400">
              <CheckCircle2 size={28} />
            </div>
            <p className="mt-3 text-center text-lg font-bold text-theme">Готово</p>
            <p className="mt-1 text-center text-sm text-muted">Signum установлен и готов к работе.</p>
            <button type="button" className="btn-primary mt-auto w-full" onClick={() => void launch()}>
              Запустить
            </button>
            <button type="button" className="btn-secondary mt-2 w-full" onClick={() => void win.close()}>
              Закрыть
            </button>
          </div>
        )}

        {page === "error" && (
          <div className="mt-8 flex flex-1 flex-col">
            <p className="text-center text-sm font-semibold text-theme">Не удалось установить</p>
            <p className="mt-2 text-center text-sm leading-5 text-red-300">{error}</p>
            <button type="button" className="btn-primary mt-auto w-full" onClick={() => void install()}>
              Повторить
            </button>
            <button type="button" className="btn-secondary mt-2 w-full" onClick={() => void win.close()}>
              Закрыть
            </button>
          </div>
        )}
      </main>
    </div>
  );
}
