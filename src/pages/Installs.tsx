import { listen } from "@tauri-apps/api/event";
import { ArrowDown, ArrowUp, ArrowUpCircle, ArrowUpRight, CheckCircle2, CircleDashed, Copy, Download, LoaderCircle, Pause, Play, RefreshCw, RotateCcw, Trash2, X, XCircle } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import ActivationModal from "../components/ActivationModal";
import PageHeader from "../components/PageHeader";
import { hydrateActivation, takePendingPostInstallActivation } from "../lib/activation";
import { command, errorMessage } from "../lib/tauri";
import { tourInstallJob, useTourDemo, useTourSignal } from "../lib/tourStore";
import { formatMinIos } from "../lib/format";
import type { ActionHistoryEntry, AppActivation, AppUpdateFoundEvent, InstallJob, ProgressEvent } from "../types";

/** Phases the walkthrough plays back so the progress bar is not a still image. */
const TOUR_FRAMES: Array<[number, InstallJob["phase"]]> = [
  [18, "download"],
  [46, "download"],
  [68, "sign"],
  [88, "install"],
];

function statusLabel(job: InstallJob): string {
  const exportJob = job.kind === "export";
  if (job.status === "running") {
    if (job.phase === "auth" || job.phase === "cert" || job.phase === "sign") return "Подпись";
    if (job.phase === "install") return "Установка";
    if (job.phase === "device") return "Устройство";
    if (exportJob) return "Загрузка";
  }
  const labels: Record<InstallJob["status"], string> = exportJob
    ? { queued: "В очереди", running: "Загрузка", done: "Сохранено", failed: "Ошибка", cancelled: "Отменено" }
    : { queued: "В очереди", running: "Выполняется", done: "Установлено", failed: "Ошибка", cancelled: "Отменено" };
  return labels[job.status];
}

function historyStatusLabel(status: string): string {
  const labels: Record<string, string> = {
    queued: "В очереди",
    running: "Выполняется",
    done: "Готово",
    failed: "Ошибка",
    cancelled: "Отменено",
  };
  return labels[status] ?? status;
}

export default function Installs() {
  const [jobs, setJobs] = useState<InstallJob[]>([]);
  const [history, setHistory] = useState<ActionHistoryEntry[]>([]);
  const [queuePaused, setQueuePaused] = useState(false);
  const [queueBusy, setQueueBusy] = useState(false);
  const [queueError, setQueueError] = useState("");
  const [activation, setActivation] = useState<{ appTitle: string; data: AppActivation } | null>(null);
  const shownJobIds = useRef(new Set<string>());

  const tryShowPostInstallActivation = useCallback((jobId: string, job?: InstallJob) => {
    if (shownJobIds.current.has(jobId)) return;
    if (job && job.kind === "export") return;
    const pending = takePendingPostInstallActivation(jobId);
    if (!pending) return;
    shownJobIds.current.add(jobId);
                                                                         
    void hydrateActivation(pending.activation, true)
      .then(data => setActivation({ appTitle: pending.appTitle, data }))
      .catch(() => setActivation({ appTitle: pending.appTitle, data: pending.activation }));
  }, []);

  const load = useCallback(() => Promise.all([
    command<InstallJob[]>("list_install_jobs").then(next => {
      setJobs(next);
      for (const job of next) {
        if (job.status === "done") tryShowPostInstallActivation(job.id, job);
      }
    }).catch(() => undefined),
    command<ActionHistoryEntry[]>("list_action_history").then(setHistory).catch(() => undefined),
    command<{ paused: boolean }>("get_install_queue_status").then(status => setQueuePaused(status.paused)).catch(() => undefined),
  ]), [tryShowPostInstallActivation]);

  useEffect(() => {
    void load();
    const timer = window.setInterval(() => void load(), 3000);
    const unlisten = listen<ProgressEvent>("install-progress", ({ payload }) => {
      setJobs(current => {
        const found = current.find(j => j.id === payload.job_id);
        if (!found) { void load(); return current; }
        return current.map(job => job.id === payload.job_id ? { ...job, phase: payload.phase, percent: payload.percent, message: payload.message, status: payload.status, error: payload.error } : job);
      });
      if (payload.status === "done") {
        tryShowPostInstallActivation(payload.job_id);
      }
    });
    return () => { window.clearInterval(timer); void unlisten.then(fn => fn()); };
  }, [load, tryShowPostInstallActivation]);

  const tourDemo = useTourDemo();
  const tourSignal = useTourSignal();
  const [tourJob, setTourJob] = useState<InstallJob | null>(null);

  useEffect(() => {
    if (!tourDemo) {
      setTourJob(null);
      return;
    }
    if (tourSignal.name === "job-done") {
      setTourJob({ ...tourInstallJob, status: "done", phase: "install", percent: 100 });
      return;
    }
    if (tourSignal.name !== "job-run") return;
    setTourJob({ ...tourInstallJob, percent: 0 });
    let cancelled = false;
    const timers: number[] = [];
    TOUR_FRAMES.forEach(([percent, phase], i) => {
      timers.push(window.setTimeout(() => {
        if (!cancelled) setTourJob({ ...tourInstallJob, phase, percent });
      }, 480 * (i + 1)));
    });
    timers.push(window.setTimeout(() => {
      if (!cancelled) setTourJob({ ...tourInstallJob, status: "done", phase: "install", percent: 100 });
    }, 480 * (TOUR_FRAMES.length + 1)));
    return () => {
      cancelled = true;
      timers.forEach(window.clearTimeout);
    };
  }, [tourDemo, tourSignal]);

  const [updates, setUpdates] = useState<AppUpdateFoundEvent[]>([]);
  const [updatesBusy, setUpdatesBusy] = useState(false);
  const [updatesError, setUpdatesError] = useState("");
  const [jobFilter, setJobFilter] = useState<"all" | "active" | "failed" | "done">("all");

  const loadUpdates = useCallback(async () => {
    setUpdatesBusy(true);
    setUpdatesError("");
    try {
      setUpdates(await command<AppUpdateFoundEvent[]>("list_app_updates"));
    } catch (e) {
      setUpdatesError(errorMessage(e));
      setUpdates([]);
    } finally {
      setUpdatesBusy(false);
    }
  }, []);

  useEffect(() => { void loadUpdates(); }, [loadUpdates]);

  async function installUpdates(items: AppUpdateFoundEvent[]) {
    if (items.length === 0 || updatesBusy) return;
    setUpdatesBusy(true);
    setUpdatesError("");
    try {
      const result = await command<{ jobIds: string[]; skipped: { appId: string; reason: string }[] }>(
        "enqueue_catalog_installs",
        { input: { appIds: items.map(item => item.app_id) } },
      );
      if (result.jobIds.length === 0) {
        setUpdatesError(result.skipped[0]?.reason || "Не удалось поставить обновления в очередь.");
        return;
      }
      setUpdates(prev => prev.filter(item => !items.some(queued => queued.app_id === item.app_id)));
      await load();
    } catch (e) {
      setUpdatesError(errorMessage(e));
    } finally {
      setUpdatesBusy(false);
    }
  }

  const shownJobs = useMemo(() => (tourJob ? [tourJob, ...jobs] : jobs), [jobs, tourJob]);
  const filteredJobs = useMemo(() => shownJobs.filter(job => {
    if (jobFilter === "active") return job.status === "queued" || job.status === "running";
    if (jobFilter === "failed") return job.status === "failed";
    if (jobFilter === "done") return job.status === "done" || job.status === "cancelled";
    return true;
  }), [jobFilter, shownJobs]);
  const active = useMemo(
    () => shownJobs.filter(j => j.status === "running" || j.status === "queued").length,
    [shownJobs],
  );

  async function manageQueue(name: string, args?: Record<string, unknown>) {
    setQueueBusy(true); setQueueError("");
    try {
      const result = await command<unknown>(name, args);
      if (name === "retry_failed_install_jobs" && Array.isArray(result) && result.length) setQueueError(result.join("\n"));
      await load();
    } catch (error) { setQueueError(errorMessage(error)); }
    finally { setQueueBusy(false); }
  }

  return <div className="mx-auto max-w-6xl p-7 lg:p-9">
    <PageHeader
      title="Установки"
      subtitle={active
        ? `${active} активных задач`
        : updates.length > 0
          ? `${updates.length} приложений на iPhone можно обновить`
          : "Очередь загрузок и установок"}
      actions={
        <div className="flex flex-wrap gap-2">
          {updates.length > 0 && (
            <button className="btn-primary" disabled={updatesBusy} onClick={() => void installUpdates(updates)}>
              {updatesBusy ? <LoaderCircle size={15} className="animate-spin" /> : <ArrowUpRight size={15} />}
              Обновить всё
            </button>
          )}
          <button className="btn-secondary" disabled={queueBusy} onClick={() => void manageQueue("set_install_queue_paused", { paused: !queuePaused })}>
            {queuePaused ? <Play size={15} /> : <Pause size={15} />}{queuePaused ? "Продолжить очередь" : "Пауза после текущей"}
          </button>
          {jobs.some(job => job.status === "failed" && job.retryable) && <button className="btn-secondary" disabled={queueBusy} onClick={() => void manageQueue("retry_failed_install_jobs")}><RotateCcw size={15} />Повторить все ошибки</button>}
          {jobs.some(job => job.status === "queued") && <button className="btn-secondary" disabled={queueBusy} onClick={() => void manageQueue("cancel_pending_install_jobs")}><X size={15} />Отменить ожидающие</button>}
          <button className="btn-secondary" onClick={() => { void load(); void loadUpdates(); }}>
            <RefreshCw size={15} />Обновить
          </button>
        </div>
      }
    />

    {queuePaused && <p role="status" className="panel mt-5 p-4 text-sm text-amber-200">Очередь на паузе. Текущая операция завершится, остальные будут ждать нажатия «Продолжить очередь».</p>}
    {queueError && <p role="alert" className="mt-4 whitespace-pre-wrap text-sm text-rose-300">{queueError}</p>}

    {updatesError && <p className="mt-6 text-xs leading-5 text-rose-300">{updatesError}</p>}

    {updates.length > 0 && (
      <section className="mt-8">
        <h2 className="mb-4 flex items-center gap-2 font-bold">
          <ArrowUpCircle size={17} className="text-brand-300" />
          Доступны обновления
          <span className="text-[11px] font-normal text-slate-600">{updates.length}</span>
        </h2>
        <div className="space-y-2">
          {updates.map(item => (
            <div key={item.app_id} className="panel flex items-center justify-between gap-4 p-4">
              <div className="min-w-0">
                <div className="truncate text-sm font-semibold text-slate-100">{item.title}</div>
                <div className="mt-0.5 text-xs text-slate-500">
                  v{item.installed_version} <span className="text-slate-600">→</span>{" "}
                  <span className="text-brand-300">v{item.remote_version}</span>
                  {formatMinIos(item.min_ios) && <span className="ml-2 text-slate-600">{formatMinIos(item.min_ios)}</span>}
                </div>
              </div>
              <button
                className="btn-secondary shrink-0"
                disabled={updatesBusy}
                onClick={() => void installUpdates([item])}
              >
                Обновить
              </button>
            </div>
          ))}
        </div>
      </section>
    )}

    <div className="mt-8">
      {shownJobs.length > 0 && (
        <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
          <div className="flex flex-wrap gap-2">
            {([["all", "Все"], ["active", "Активные"], ["failed", "Ошибки"], ["done", "Завершённые"]] as const).map(([value, label]) => (
              <button key={value} type="button" onClick={() => setJobFilter(value)} className={`rounded-lg px-3 py-1.5 text-[11px] font-semibold transition ${jobFilter === value ? "bg-brand-500/20 text-brand-200 ring-1 ring-brand-400/30" : "bg-white/[0.04] text-slate-500 hover:bg-white/[0.07] hover:text-white"}`}>
                {label}
              </button>
            ))}
          </div>
          {jobs.some(job => job.status === "done" || job.status === "failed" || job.status === "cancelled") && (
            <button type="button" className="btn-secondary px-2.5 py-1.5 text-[11px]" onClick={() => void command<void>("clear_finished_install_jobs").then(load)}>
              <Trash2 size={13} />Очистить завершённые
            </button>
          )}
        </div>
      )}
      {filteredJobs.length === 0 ? <div className="panel flex min-h-72 flex-col items-center justify-center text-center"><div className="rounded-2xl bg-white/5 p-4 text-slate-600"><Download size={30} /></div><h2 className="mt-4 font-bold">{shownJobs.length ? "Ничего не найдено" : "Очередь пуста"}</h2><p className="mt-2 max-w-md text-sm text-slate-500">{shownJobs.length ? "Для этого фильтра задач нет." : "Выберите приложение в каталоге, скачайте его или укажите файл с компьютера. Здесь же появятся обновления для того, что уже стоит на iPhone."}</p></div> : filteredJobs.map(job => <div className="mb-3" key={job.id}><JobRow job={job} demo={job.id === tourInstallJob.id} onChanged={() => void load()} /></div>)}
    </div>
    {history.length > 0 && (
      <section className="mt-10">
        <h2 className="mb-4 font-bold">История действий</h2>
        <div className="space-y-2">
          {history.slice(0, 20).map(entry => (
            <div key={entry.id} className="panel flex items-center justify-between gap-4 p-4">
              <div className="min-w-0">
                <div className="text-sm font-semibold text-slate-100">{entry.title}</div>
                <div className="mt-0.5 truncate text-xs text-slate-600">{entry.subtitle}</div>
                {entry.account_email && <div className="mt-1 text-[10px] text-slate-700">{entry.account_email}</div>}
              </div>
              <div className="shrink-0 text-right">
                <div className={`text-[10px] font-bold uppercase ${entry.status === "done" ? "text-emerald-300" : entry.status === "failed" ? "text-rose-300" : "text-slate-500"}`}>{historyStatusLabel(entry.status)}</div>
                <div className="mt-1 text-[10px] text-slate-700">{new Date(entry.finished_at * 1000).toLocaleString("ru-RU")}</div>
              </div>
            </div>
          ))}
        </div>
      </section>
    )}
    {activation && (
      <ActivationModal
        appTitle={activation.appTitle}
        activation={activation.data}
        onClose={() => setActivation(null)}
      />
    )}
  </div>;
}

function JobRow({ job, onChanged, demo = false }: { job: InstallJob; onChanged: () => void; demo?: boolean }) {
  const [copied, setCopied] = useState(false);
  const [busy, setBusy] = useState(false);
  const Icon = job.status === "done" ? CheckCircle2 : job.status === "failed" ? XCircle : job.status === "cancelled" ? X : job.status === "running" ? LoaderCircle : CircleDashed;
  const tone = job.status === "done" ? "text-emerald-400" : job.status === "failed" ? "text-rose-400" : job.status === "cancelled" ? "text-slate-500" : job.status === "running" ? "text-brand-300" : "text-slate-500";
  const exportJob = job.kind === "export" || job.kind === "open_exports";
  const details = job.error
    ? exportJob && job.error.startsWith("Не удалось выполнить операцию. Проверьте Apple ID, подключение iPhone")
      ? "Не удалось скачать файл. Проверьте интернет, вход в Apple ID и доступ к папке экспорта. Подключать iPhone для скачивания не нужно."
      : errorMessage(job.error)
    : job.message;
  const actionHint = job.error ? errorHint(details, exportJob) : "";
  const canCancel = job.status === "queued" || job.status === "running";
  const indeterminate = job.status === "running" && job.percent < 1;

  async function cancel() {
    setBusy(true);
    try {
      await command<void>("cancel_install_job", { jobId: job.id });
      onChanged();
    } catch (e) {
      window.alert(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function retry() {
    setBusy(true);
    try {
      await command<string>("retry_install_job", { jobId: job.id });
      onChanged();
    } catch (e) {
      window.alert(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function move(direction: number) {
    setBusy(true);
    try { await command("move_install_job", { jobId: job.id, direction }); onChanged(); }
    catch (error) { window.alert(errorMessage(error)); }
    finally { setBusy(false); }
  }

  async function copyError() {
    if (!details) return;
    try {
      await navigator.clipboard.writeText(details);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      const area = document.createElement("textarea");
      area.value = details;
      area.style.position = "fixed";
      area.style.left = "-9999px";
      document.body.appendChild(area);
      area.select();
      document.execCommand("copy");
      document.body.removeChild(area);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    }
  }

  return <div className="panel p-5" data-tour={demo ? "install-job" : undefined}>
    {job.status === "queued" && !demo && <div className="mb-3 flex justify-end gap-2">
      <button className="btn-secondary px-2.5 py-1.5 text-[11px]" disabled={busy} onClick={() => void move(-1)} aria-label={`Поднять ${job.title} в очереди`}><ArrowUp size={13} />Выше</button>
      <button className="btn-secondary px-2.5 py-1.5 text-[11px]" disabled={busy} onClick={() => void move(1)} aria-label={`Опустить ${job.title} в очереди`}><ArrowDown size={13} />Ниже</button>
    </div>}
    <div className="flex items-start gap-4"><div className={`mt-0.5 rounded-xl bg-white/5 p-3 ${tone}`}><Icon size={20} className={job.status === "running" ? "animate-spin" : ""} /></div>
      <div className="min-w-0 flex-1"><div className="flex items-center justify-between gap-4"><div><h3 className="font-semibold text-slate-100">{job.title}</h3><p className="mt-0.5 text-xs text-slate-600">{job.subtitle}</p></div><div className="flex shrink-0 items-center gap-2"><div className={`text-xs font-semibold ${tone}`}>{statusLabel(job)} · {Math.round(job.percent)}%</div>{canCancel && <button disabled={busy} onClick={() => void cancel()} className="btn-secondary px-2.5 py-1.5 text-[11px]"><X size={13} />Отменить</button>}{job.status === "failed" && job.retryable && <button disabled={busy} onClick={() => void retry()} className="btn-secondary px-2.5 py-1.5 text-[11px]"><RotateCcw size={13} />Повторить</button>}{job.kind === "export" && job.status === "done" && <button type="button" onClick={() => void command<void>("open_exports_folder")} className="btn-secondary px-2.5 py-1.5 text-[11px]">Папка</button>}</div></div>
        <div className="mt-4 h-1.5 overflow-hidden rounded-full bg-white/[0.055]">
          {indeterminate ? (
            <div className="signum-progress-indeterminate h-full w-2/5 rounded-full bg-brand-gradient" />
          ) : (
            <div className={`h-full rounded-full transition-all duration-300 ${job.status === "failed" ? "bg-rose-500" : job.status === "done" ? "bg-emerald-400" : job.status === "cancelled" ? "bg-slate-600" : "bg-brand-gradient"}`} style={{ width: `${job.percent}%` }} />
          )}
        </div>
        {job.status === "failed" && details ? (
          <div className="mt-3 rounded-xl border border-rose-400/15 bg-rose-500/[0.06] p-3">
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0 flex-1">
                <p className="select-text whitespace-pre-wrap break-words text-xs leading-5 text-rose-100/90">{details}</p>
                {actionHint && <p className="mt-2 border-t border-rose-400/15 pt-2 text-xs font-medium text-rose-200">Что сделать: {actionHint}</p>}
              </div>
              <button type="button" onClick={() => void copyError()} className="btn-secondary shrink-0 px-2.5 py-1.5 text-[11px]">
                <Copy size={13} />{copied ? "Скопировано" : "Копировать"}
              </button>
            </div>
          </div>
        ) : (
          <div className="mt-2.5 text-[11px]">
            <span className={`min-w-0 truncate ${job.status === "running" ? "text-slate-300" : "text-slate-500"}`}>{details}</span>

          </div>
        )}


      </div>
    </div>
  </div>;
}

function errorHint(message: string, exportJob = false): string {
  const value = message.toLowerCase();
  if (!exportJob && (value.includes("iphone") || value.includes("устройств"))) return "разблокируйте iPhone, проверьте кабель и нажмите «Доверять устройству».";
  if (value.includes("apple id") || value.includes("сесс") || value.includes("войдите") || value.includes("2fa")) return "откройте «Аккаунт», обновите вход и повторите задачу.";
  if (value.includes("мест") || value.includes("disk") || value.includes("space")) return "освободите место на компьютере или выберите другую папку загрузок.";
  if (value.includes("сеть") || value.includes("интернет") || value.includes("network") || value.includes("timeout") || value.includes("соединен")) return "проверьте интернет и повторите задачу.";
  if (!exportJob && (value.includes("совместим") || value.includes("ios"))) return "проверьте совместимость версии с iOS устройства.";
  return "повторите задачу; если ошибка остаётся, скопируйте текст и отправьте его в поддержку.";
}
