import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LoaderCircle, X } from "lucide-react";
import { useEffect, useState } from "react";
import signumIcon from "../../assets/signum-icon.png";
import Checkbox from "../components/Checkbox";
import DragRegion from "../components/DragRegion";

type Page = "confirm" | "busy" | "error";

type UninstallInfo = {
  version: string;
  target: string;
};

export default function UninstallApp() {
  const win = getCurrentWindow();
  const [info, setInfo] = useState<UninstallInfo | null>(null);
  const [deleteData, setDeleteData] = useState(true);
  const [page, setPage] = useState<Page>("confirm");
  const [error, setError] = useState("");

  useEffect(() => {
    void invoke<UninstallInfo>("uninstall_info").then(setInfo).catch(() => undefined);
  }, []);

  async function uninstall() {
    setPage("busy");
    setError("");
    try {
      await invoke("start_uninstall", { deleteData });
    } catch (e) {
      setError(typeof e === "string" ? e : "Не удалось удалить Signum.");
      setPage("error");
    }
  }

  const busy = page === "busy";

  return (
    <div className="app-shell flex h-full flex-col">
      <header className="app-header relative z-30 flex h-11 shrink-0 items-stretch">
        <DragRegion className="flex h-11 min-w-0 flex-1 cursor-grab items-center px-4 active:cursor-grabbing">
          <span className="pointer-events-none text-xs font-medium text-muted">Удаление</span>
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

        {page === "confirm" && (
          <div className="mt-6 flex min-h-0 flex-1 flex-col">
            <p className="text-center text-sm text-theme">Удалить Signum с этого компьютера?</p>
            {info?.target && (
              <p className="mt-2 break-all text-center text-[11px] leading-4 text-muted">{info.target}</p>
            )}
            <div className="panel mt-5 px-4 py-3">
              <Checkbox
                checked={deleteData}
                onChange={setDeleteData}
                label="Удалить данные приложения"
                description="Вход в Apple ID и сохранённые данные тоже будут стёрты."
              />
            </div>
            <button type="button" className="btn-primary mt-auto w-full" onClick={() => void uninstall()}>
              Удалить
            </button>
            <button type="button" className="btn-secondary mt-2 w-full" onClick={() => void win.close()}>
              Отмена
            </button>
          </div>
        )}

        {page === "busy" && (
          <div className="mt-8 flex flex-1 flex-col">
            <p className="text-center text-sm font-semibold text-theme">Удаляем Signum</p>
            <p className="mt-2 text-center text-xs text-muted">
              {deleteData ? "Стираем программу и данные…" : "Удаляем программу…"}
            </p>
            <div className="mt-auto flex justify-center text-muted">
              <LoaderCircle size={18} className="animate-spin" />
            </div>
          </div>
        )}

        {page === "error" && (
          <div className="mt-8 flex flex-1 flex-col">
            <p className="text-center text-sm font-semibold text-theme">Не удалось удалить</p>
            <p className="mt-2 text-center text-sm leading-5 text-red-300">{error}</p>
            <button type="button" className="btn-primary mt-auto w-full" onClick={() => void uninstall()}>
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
