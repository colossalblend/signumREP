import { ArrowUpRight, Check, QrCode, RotateCcw, Star, Store } from "lucide-react";
import { memo } from "react";
import { resolveAppActivation } from "../lib/activation";
import { formatCatalogCardVersion } from "../lib/format";
import type { CatalogApp } from "../types";

const badgeStyles: Record<string, string> = {
  store: "border-emerald-400/20 bg-emerald-400/10 text-emerald-300",
  restore: "border-amber-400/20 bg-amber-400/10 text-amber-300",
};
const badgeLabels: Record<string, string> = { store: "App Store", restore: "Из покупок" };

// Callbacks take the app so the parent can hand over stable references — with
// fresh arrow functions per render `memo` would never skip anything, and every
// click in multi-select mode re-rendered the whole catalog.
function AppCard(
  { app, onClick, dataTour, favorite, onToggleFavorite, selectable, selected }: {
    app: CatalogApp;
    onClick: (app: CatalogApp) => void;
    dataTour?: string;
    favorite?: boolean;
    onToggleFavorite?: (app: CatalogApp) => void;
    selectable?: boolean;
    selected?: boolean;
  },
) {
  const badge = app.in_store ? "store" : "restore";
  const BadgeIcon = badge === "store" ? Store : RotateCcw;
  const accent = app.color ?? "#5b6cf0";
  const hasActivation = !!resolveAppActivation(app.activation);

  return (
    <button
      onClick={() => onClick(app)}
      data-tour={dataTour}
      aria-pressed={selectable ? !!selected : undefined}
      className={`panel group relative flex min-h-[208px] flex-col overflow-hidden p-5 text-left transition duration-200 hover:border-brand-400/30 ${selected ? "ring-2 ring-brand-400" : ""}`}
    >
      {/* Цвет приложения — тонкой полосой сверху, а не размытым пятном. */}
      <span aria-hidden className="pointer-events-none absolute inset-x-0 top-0 h-[3px] opacity-70" style={{ background: accent }} />
      <div className="flex items-start justify-between gap-3">
        {app.icon_url ? (
          <img
            key={app.icon_url}
            src={app.icon_url}
            alt=""
            draggable={false}
            className="h-16 w-16 shrink-0 rounded-[18px] object-cover shadow-md ring-1 ring-black/10 dark:ring-white/10"
          />
        ) : (
          <div
            aria-hidden
            className="on-color grid h-16 w-16 shrink-0 place-items-center rounded-[18px] text-2xl font-bold text-white shadow-md"
            style={{ background: accent }}
          >
            {app.title.slice(0, 1)}
          </div>
        )}
        <div className="flex flex-col items-end gap-1.5">
          <span className={`inline-flex items-center gap-1.5 rounded-md border px-2 py-0.5 text-[10px] font-bold uppercase tracking-wider ${badgeStyles[badge]}`}>
            <BadgeIcon size={11} />{badgeLabels[badge]}
          </span>
          {hasActivation && (
            <span className="inline-flex items-center gap-1 rounded-md border border-violet-400/20 bg-violet-400/10 px-2 py-0.5 text-[10px] font-bold uppercase tracking-wider text-violet-300">
              <QrCode size={11} />Активация
            </span>
          )}
        </div>
      </div>

      <div className="mt-4 min-w-0">
        <h3 className="line-clamp-2 text-base font-bold leading-snug tracking-tight text-theme">{app.title}</h3>
        <p className="mt-1 text-xs tabular-nums text-muted">{formatCatalogCardVersion(app)}</p>
      </div>

      <div className="mt-auto flex items-center justify-between pt-4">
        {onToggleFavorite ? (
          // Not a <button>: the whole card already is one, and nesting them is invalid.
          <span
            role="button"
            tabIndex={-1}
            aria-label={favorite ? "Убрать из избранного" : "В избранное"}
            title={favorite ? "Убрать из избранного" : "В избранное"}
            onClick={event => { event.stopPropagation(); onToggleFavorite(app); }}
            className={`grid h-8 w-8 place-items-center rounded-lg transition ${favorite ? "bg-amber-400/15 text-amber-300" : "bg-white/[0.055] text-slate-500 hover:bg-amber-400/15 hover:text-amber-300"}`}
          >
            <Star size={15} className={favorite ? "fill-amber-300" : ""} />
          </span>
        ) : <span />}
        {selectable ? (
          <span className={`grid h-8 w-8 place-items-center rounded-lg transition ${selected ? "bg-brand-500 text-white" : "bg-white/[0.055] text-slate-500"}`}>
            {selected ? <Check size={15} /> : <span className="h-3.5 w-3.5 rounded border border-current" />}
          </span>
        ) : (
          <span className="grid h-8 w-8 place-items-center rounded-lg bg-white/[0.055] text-brand-300 transition group-hover:bg-brand-500 group-hover:text-white"><ArrowUpRight size={15} /></span>
        )}
      </div>
    </button>
  );
}

export default memo(AppCard);
