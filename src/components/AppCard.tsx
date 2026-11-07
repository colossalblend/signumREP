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
      className={`panel group relative min-h-[270px] overflow-hidden p-5 text-left transition duration-300 hover:-translate-y-1 hover:border-white/[0.13] hover:bg-white/[0.055] ${selected ? "ring-2 ring-brand-400" : ""}`}
    >
      <div className="pointer-events-none absolute -right-12 -top-12 h-40 w-40 rounded-full opacity-20 blur-3xl transition group-hover:opacity-35" style={{ background: accent }} />
      <div className="flex items-start justify-between gap-3">
        <div className="relative h-[86px] w-[86px]">
          <div className="absolute inset-2 translate-y-3 rounded-[24px] opacity-30 blur-xl" style={{ background: accent }} />
          <div className="absolute inset-1 translate-x-2 translate-y-2 rounded-[22px] bg-black/35" />
          {app.icon_url ? (
            <img
              key={app.icon_url}
              src={app.icon_url}
              alt=""
              draggable={false}
              className="relative h-[78px] w-[78px] rounded-[20px] object-cover drop-shadow-icon-lg ring-1 ring-white/20"
            />
          ) : (
            <div className="relative grid h-[78px] w-[78px] place-items-center rounded-[20px] bg-brand-gradient text-3xl font-black drop-shadow-icon-lg">{app.title.slice(0, 1)}</div>
          )}
        </div>
        <div className="flex flex-col items-end gap-1.5">
          <span className={`inline-flex items-center gap-1.5 rounded-lg border px-2.5 py-1 text-[10px] font-bold uppercase tracking-wider ${badgeStyles[badge]}`}>
            <BadgeIcon size={11} />{badgeLabels[badge]}
          </span>
          {hasActivation && (
            <span className="inline-flex items-center gap-1 rounded-lg border border-violet-400/20 bg-violet-400/10 px-2 py-1 text-[10px] font-bold uppercase tracking-wider text-violet-300">
              <QrCode size={11} />Активация
            </span>
          )}
        </div>
      </div>

      <div className="mt-6">
        <h3 className="line-clamp-2 text-lg font-bold tracking-tight text-white">{app.title}</h3>
        <p className="mt-1 text-xs text-slate-500">{formatCatalogCardVersion(app)}</p>
      </div>
      {onToggleFavorite && (
        // Not a <button>: the whole card already is one, and nesting them is invalid.
        <span
          role="button"
          tabIndex={-1}
          aria-label={favorite ? "Убрать из избранного" : "В избранное"}
          onClick={event => { event.stopPropagation(); onToggleFavorite(app); }}
          className={`absolute bottom-5 left-5 grid h-8 w-8 place-items-center rounded-lg transition ${favorite ? "bg-amber-400/15 text-amber-300" : "bg-white/[0.055] text-slate-500 hover:bg-amber-400/15 hover:text-amber-300"}`}
        >
          <Star size={15} className={favorite ? "fill-amber-300" : ""} />
        </span>
      )}
      <div className="absolute bottom-5 right-5">
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
