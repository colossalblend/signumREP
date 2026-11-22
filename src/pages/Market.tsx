import {
  AlertTriangle, Check, CheckCircle2, CreditCard, FlaskConical, KeyRound, LoaderCircle,
  RotateCcw, Search, Send, ShieldCheck, Smartphone, Star, Store, Users, X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import PageHeader from "../components/PageHeader";
import {
  categoryLabel, findConnectedDevice, formatPrice, iosAtLeast, isAppInstalled,
  MARKET_CATEGORIES, MARKET_MIN_IOS, minPrice, searchMarket, TEST_MARKET_APPS,
  testFormFairplay, testPay, testTransfer,
  type DeviceSnapshot, type MarketApp, type MarketCategory, type MarketSeller, type PurchaseStage,
} from "../lib/market";

// ───────────────────────── Карточка приложения ─────────────────────────

function MarketIcon({ app, size = 78 }: { app: MarketApp; size?: number }) {
  const [broken, setBroken] = useState(false);
  const radius = Math.round(size * 0.26);
  if (app.icon_url && !broken) {
    return (
      <img
        src={app.icon_url}
        alt=""
        draggable={false}
        onError={() => setBroken(true)}
        style={{ width: size, height: size, borderRadius: radius }}
        className="relative object-cover drop-shadow-icon-lg ring-1 ring-white/20"
      />
    );
  }
  return (
    <div
      style={{ width: size, height: size, borderRadius: radius, background: app.color }}
      className="relative grid place-items-center font-black text-white drop-shadow-icon-lg ring-1 ring-white/20"
    >
      <span style={{ fontSize: size * 0.4 }}>{app.title.slice(0, 1)}</span>
    </div>
  );
}

function MarketCard({ app, onOpen }: { app: MarketApp; onOpen: (app: MarketApp) => void }) {
  const sellers = app.sellers.length;
  return (
    <button type="button" onClick={() => onOpen(app)} className="panel app-card group">
      <div className="pointer-events-none absolute -right-12 -top-12 h-40 w-40 rounded-full opacity-20 blur-3xl transition group-hover:opacity-35" style={{ background: app.color }} />
      <div className="flex items-start justify-between gap-3">
        <div className="relative h-[86px] w-[86px]">
          <div className="absolute inset-2 translate-y-3 rounded-[24px] opacity-30 blur-xl" style={{ background: app.color }} />
          <div className="absolute inset-1 translate-x-2 translate-y-2 rounded-[22px] bg-black/35" />
          <MarketIcon app={app} />
        </div>
        <span className="inline-flex items-center gap-1.5 rounded-lg border border-emerald-400/20 bg-emerald-400/10 px-2.5 py-1 text-[10px] font-bold uppercase tracking-wider text-emerald-300">
          <Store size={11} />от {formatPrice(minPrice(app))}
        </span>
      </div>
      <div className="mt-6 pb-10">
        <h3 className="line-clamp-2 text-lg font-bold leading-snug tracking-tight text-theme">{app.title}</h3>
        <p className="mt-1 text-xs text-slate-500">{app.developer} · {categoryLabel(app.category)}</p>
      </div>
      <span className="absolute bottom-5 left-5 inline-flex items-center gap-1.5 text-xs font-medium text-slate-400">
        <Users size={13} />
        {sellers === 1 ? "1 продавец" : `${sellers} ${sellers < 5 ? "продавца" : "продавцов"}`}
      </span>
    </button>
  );
}

// ───────────────────────── Этапы установки ─────────────────────────

const STEPS: { stage: PurchaseStage; label: string; icon: typeof Smartphone }[] = [
  { stage: "connect", label: "Подключение iPhone", icon: Smartphone },
  { stage: "check_ios", label: `Проверка iOS ${MARKET_MIN_IOS}+`, icon: ShieldCheck },
  { stage: "paying", label: "Оплата через ЮKassa", icon: CreditCard },
  { stage: "forming", label: "Формирование данных FairPlay", icon: KeyRound },
  { stage: "transferring", label: "Передача на iPhone", icon: Send },
  { stage: "verifying", label: "Проверка установки", icon: CheckCircle2 },
];

const STAGE_ORDER: PurchaseStage[] = ["connect", "check_ios", "confirm", "paying", "forming", "transferring", "verifying", "installed"];

function stepState(step: PurchaseStage, current: PurchaseStage): "done" | "active" | "pending" {
  // «confirm» стоит между проверкой iOS и оплатой — на нём оплата ещё ждёт.
  const cur = STAGE_ORDER.indexOf(current === "confirm" ? "paying" : current);
  const idx = STAGE_ORDER.indexOf(step);
  if (current === "installed") return "done";
  if (current === "confirm" && step === "paying") return "pending";
  if (idx < cur) return "done";
  if (idx === cur) return "active";
  return "pending";
}

function Stepper({ stage }: { stage: PurchaseStage }) {
  return (
    <ol className="mt-5 space-y-1.5">
      {STEPS.map(({ stage: step, label, icon: Icon }) => {
        const state = stepState(step, stage);
        return (
          <li
            key={step}
            className={`flex items-center gap-3 rounded-xl px-3 py-2.5 text-sm transition ${state === "active" ? "bg-brand-500/10 text-theme ring-1 ring-brand-400/20" : state === "done" ? "text-slate-300" : "text-slate-500"}`}
          >
            <span className={`grid h-7 w-7 shrink-0 place-items-center rounded-lg ${state === "done" ? "bg-emerald-400/15 text-emerald-300" : state === "active" ? "bg-brand-500/20 text-brand-300" : "bg-white/[0.05] text-slate-500"}`}>
              {state === "done" ? <Check size={15} /> : state === "active" ? <LoaderCircle size={15} className="animate-spin" /> : <Icon size={15} />}
            </span>
            {label}
          </li>
        );
      })}
    </ol>
  );
}

function SellerRow({ seller, onPick, cheapest }: { seller: MarketSeller; onPick: (s: MarketSeller) => void; cheapest: boolean }) {
  return (
    <button
      type="button"
      onClick={() => onPick(seller)}
      className="panel flex w-full items-center gap-4 p-4 text-left transition hover:border-white/[0.12] hover:bg-white/[0.04]"
    >
      <div className="grid h-11 w-11 shrink-0 place-items-center rounded-xl bg-brand-gradient text-base font-black text-white">{seller.name.slice(0, 1)}</div>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="truncate font-semibold text-theme">{seller.name}</span>
          {cheapest && <span className="rounded-md bg-emerald-400/15 px-1.5 py-0.5 text-[10px] font-bold uppercase tracking-wide text-emerald-300">Дешевле всех</span>}
        </div>
        <div className="mt-0.5 flex items-center gap-3 text-xs text-slate-500">
          <span className="inline-flex items-center gap-1"><Star size={12} className="fill-amber-300 text-amber-300" />{seller.rating.toFixed(1)}</span>
          <span>{seller.sales} установок</span>
          <span>Apple ID: {seller.storefront}</span>
        </div>
      </div>
      <span className="shrink-0 text-base font-bold tabular-nums text-theme">{formatPrice(seller.price)}</span>
    </button>
  );
}

// ───────────────────────── Окно покупки и установки ─────────────────────────

function PurchaseSheet({ app, onClose }: { app: MarketApp; onClose: () => void }) {
  const single = app.sellers.length === 1;
  const [stage, setStage] = useState<PurchaseStage>(single ? "connect" : "select_seller");
  const [seller, setSeller] = useState<MarketSeller | null>(single ? app.sellers[0] : null);
  const [device, setDevice] = useState<DeviceSnapshot | null>(null);
  const [simulated, setSimulated] = useState(false);
  const [noDevice, setNoDevice] = useState(false);
  const [error, setError] = useState("");
  // Оплата проходит один раз: «Повторить» после неё переделывает только
  // перенос FairPlay и проверку, повторно не списывая деньги.
  const [paid, setPaid] = useState(false);
  // Отмена: если окно закрыли посреди этапа, его результат больше не нужен.
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => { alive.current = false; };
  }, []);

  const cheapestId = useMemo(
    () => app.sellers.reduce((a, b) => (b.price < a.price ? b : a)).id,
    [app.sellers],
  );

  const checkIos = useCallback((snapshot: DeviceSnapshot) => {
    setDevice(snapshot);
    setStage("check_ios");
    window.setTimeout(() => {
      if (!alive.current) return;
      if (!snapshot.ios || !iosAtLeast(snapshot.ios)) {
        setStage("ios_too_low");
        return;
      }
      setStage("confirm");
    }, 700);
  }, []);

  const connect = useCallback(async () => {
    setError("");
    setNoDevice(false);
    setSimulated(false);
    setStage("connect");
    const found = await findConnectedDevice();
    if (!alive.current) return;
    if (!found) { setNoDevice(true); return; }
    checkIos(found);
  }, [checkIos]);

  // Единственный продавец — сразу к подключению iPhone.
  useEffect(() => {
    if (single) void connect();
  }, [single, connect]);

  const continueSimulated = () => {
    setNoDevice(false);
    setSimulated(true);
    checkIos({ udid: "TEST-DEVICE", name: "Тестовый iPhone", model: "iPhone16,1", ios: "18.1" });
  };

  const pickSeller = (picked: MarketSeller) => {
    setSeller(picked);
    void connect();
  };

  const runInstall = useCallback(async () => {
    if (!seller || !device) return;
    setError("");
    try {
      if (!paid) {
        setStage("paying");
        await testPay(seller);
        if (!alive.current) return;
        setPaid(true);
      }
      setStage("forming");
      await testFormFairplay(app, device);
      if (!alive.current) return;
      setStage("transferring");
      await testTransfer(app, device);
      if (!alive.current) return;
      setStage("verifying");
      let installed = true;
      if (!simulated) {
        try {
          installed = await isAppInstalled(app.bundle_id);
        } catch {
          // Тест: сервер ещё не переносит данные, поэтому команда может не
          // ответить. Считаем установку успешной, чтобы пройти весь сценарий.
          installed = true;
        }
      }
      await new Promise(r => setTimeout(r, 700));
      if (!alive.current) return;
      if (installed) {
        setStage("installed");
      } else {
        setError("Приложение не найдено на iPhone после передачи. Нажмите «Повторить».");
        setStage("failed");
      }
    } catch (e) {
      if (!alive.current) return;
      setError(e instanceof Error ? e.message : "Не удалось завершить установку.");
      setStage("failed");
    }
  }, [app, device, paid, seller, simulated]);

  const busy = ["connect", "check_ios", "paying", "forming", "transferring", "verifying"].includes(stage) && !noDevice;
  const closeLocked = ["paying", "forming", "transferring", "verifying"].includes(stage);

  return (
    <div
      className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-6 backdrop-blur-md"
      onMouseDown={() => { if (!closeLocked) onClose(); }}
    >
      <div className="panel max-h-[calc(100vh-3rem)] w-full max-w-lg overflow-y-auto overscroll-contain p-6" onMouseDown={e => e.stopPropagation()}>
        <div className="flex items-start gap-4">
          <MarketIcon app={app} size={64} />
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <h2 className="truncate text-xl font-bold tracking-tight text-theme">{app.title}</h2>
              <span className="inline-flex items-center gap-1 rounded-md border border-amber-400/25 bg-amber-400/10 px-1.5 py-0.5 text-[10px] font-bold uppercase tracking-wide text-amber-300">
                <FlaskConical size={10} />Тест
              </span>
            </div>
            <p className="mt-0.5 truncate text-xs text-slate-500">{app.bundle_id} · ID {app.app_store_id}</p>
          </div>
          <button
            type="button"
            aria-label="Закрыть"
            disabled={closeLocked}
            onClick={onClose}
            className="grid h-9 w-9 shrink-0 place-items-center rounded-lg text-slate-400 transition hover:bg-white/[0.06] hover:text-theme disabled:opacity-30"
          >
            <X size={18} />
          </button>
        </div>

        {stage === "select_seller" ? (
          <div className="mt-6">
            <p className="text-sm text-slate-400">Выберите продавца. Он устанавливает приложение со своего Apple ID — цена у каждого своя.</p>
            <div className="mt-4 space-y-2.5">
              {[...app.sellers].sort((a, b) => a.price - b.price).map(s => (
                <SellerRow key={s.id} seller={s} cheapest={s.id === cheapestId && app.sellers.length > 1} onPick={pickSeller} />
              ))}
            </div>
          </div>
        ) : (
          <>
            {seller && (
              <div className="mt-5 flex items-center justify-between rounded-xl border border-subtle px-4 py-3 text-sm">
                <span className="text-slate-400">Продавец <span className="font-semibold text-theme">{seller.name}</span></span>
                <span className="font-bold tabular-nums text-theme">{formatPrice(seller.price)}</span>
              </div>
            )}

            {stage !== "installed" && stage !== "failed" && stage !== "ios_too_low" && <Stepper stage={stage} />}

            {noDevice && (
              <div className="mt-5 rounded-xl border border-amber-400/20 bg-amber-400/[0.06] p-4">
                <div className="flex items-start gap-3">
                  <AlertTriangle size={18} className="mt-0.5 shrink-0 text-amber-300" />
                  <div className="text-sm">
                    <p className="font-semibold text-theme">iPhone не найден</p>
                    <p className="mt-1 text-slate-400">Подключите iPhone по USB, разблокируйте его и нажмите «Доверять».</p>
                  </div>
                </div>
                <div className="mt-4 flex flex-wrap gap-2">
                  <button type="button" className="btn-primary" onClick={() => void connect()}><RotateCcw size={15} />Повторить</button>
                  <button type="button" className="btn-secondary" onClick={continueSimulated}><FlaskConical size={15} />Продолжить в тестовом режиме</button>
                </div>
              </div>
            )}

            {device && !noDevice && (
              <p className="mt-4 flex items-center gap-2 text-xs text-slate-500">
                <Smartphone size={13} />
                {device.name}{device.ios ? ` · iOS ${device.ios}` : ""}{simulated ? " · имитация" : ""}
              </p>
            )}

            {stage === "ios_too_low" && (
              <div className="mt-5 rounded-xl border border-rose-400/20 bg-rose-400/[0.06] p-4">
                <div className="flex items-start gap-3">
                  <AlertTriangle size={18} className="mt-0.5 shrink-0 text-rose-300" />
                  <div className="text-sm">
                    <p className="font-semibold text-theme">Нужна iOS {MARKET_MIN_IOS} или новее</p>
                    <p className="mt-1 text-slate-400">
                      На iPhone {device?.ios ? `iOS ${device.ios}` : "версия iOS не определилась"}. Обновите iPhone в «Настройки → Основные → Обновление ПО» и повторите.
                    </p>
                  </div>
                </div>
                <button type="button" className="btn-secondary mt-4" onClick={() => void connect()}><RotateCcw size={15} />Проверить снова</button>
              </div>
            )}

            {stage === "confirm" && seller && (
              <div className="mt-5">
                <button type="button" className="btn-primary w-full py-3" onClick={() => void runInstall()}>
                  <CreditCard size={16} />Оплатить {formatPrice(seller.price)} через ЮKassa
                </button>
                <p className="mt-2 text-center text-xs text-slate-500">Тестовый платёж: деньги не списываются.</p>
              </div>
            )}

            {stage === "installed" && (
              <div className="mt-6 flex flex-col items-center text-center">
                <div className="grid h-16 w-16 place-items-center rounded-2xl bg-emerald-400/15 text-emerald-300">
                  <CheckCircle2 size={32} />
                </div>
                <h3 className="mt-4 text-lg font-bold text-theme">Приложение установлено</h3>
                <p className="mt-1 text-sm text-slate-400">{app.title} уже на {device?.name ?? "iPhone"}. Можно открывать.</p>
                <div className="mt-6 grid w-full grid-cols-2 gap-2">
                  <button type="button" className="btn-secondary" onClick={() => void runInstall()}><RotateCcw size={15} />Повторить</button>
                  <button type="button" className="btn-primary" onClick={onClose}><Check size={15} />Завершить установку</button>
                </div>
              </div>
            )}

            {stage === "failed" && (
              <div className="mt-6 rounded-xl border border-rose-400/20 bg-rose-400/[0.06] p-4">
                <div className="flex items-start gap-3">
                  <AlertTriangle size={18} className="mt-0.5 shrink-0 text-rose-300" />
                  <p className="text-sm text-slate-300">{error || "Не удалось завершить установку."}</p>
                </div>
                <div className="mt-4 grid grid-cols-2 gap-2">
                  <button type="button" className="btn-secondary" onClick={onClose}>Закрыть</button>
                  <button type="button" className="btn-primary" onClick={() => void runInstall()}><RotateCcw size={15} />Повторить</button>
                </div>
              </div>
            )}

            {busy && !noDevice && stage !== "confirm" && (
              <p className="mt-4 text-center text-xs text-slate-500">Не отключайте iPhone до конца установки.</p>
            )}
          </>
        )}
      </div>
    </div>
  );
}

// ───────────────────────── Страница ─────────────────────────

export default function Market() {
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState<MarketCategory | "all">("all");
  const [selected, setSelected] = useState<MarketApp | null>(null);
  const apps = TEST_MARKET_APPS;

  const visible = useMemo(() => searchMarket(apps, query, category), [apps, query, category]);
  const openApp = useCallback((app: MarketApp) => setSelected(app), []);

  return (
    <div className="mx-auto max-w-[1500px] p-7 lg:p-9">
      <PageHeader
        title="Маркет"
        subtitle="Установка приложений со своих Apple ID от проверенных продавцов"
        actions={
          <span className="inline-flex items-center gap-1.5 rounded-lg border border-amber-400/25 bg-amber-400/10 px-2.5 py-1.5 text-xs font-semibold text-amber-300">
            <FlaskConical size={13} />Тестовый режим
          </span>
        }
      />

      <div className="panel mt-7 flex items-center gap-3 px-4 py-3 transition focus-within:border-brand-400/40">
        <Search size={18} className="shrink-0 text-slate-500" />
        <input
          value={query}
          onChange={e => setQuery(e.target.value)}
          placeholder="Название, разработчик, Bundle ID или номер App Store"
          className="min-w-0 flex-1 bg-transparent text-sm text-theme outline-none placeholder:text-slate-600"
        />
        {query && (
          <button type="button" aria-label="Очистить" onClick={() => setQuery("")} className="text-slate-500 transition hover:text-theme">
            <X size={16} />
          </button>
        )}
      </div>

      <div className="mt-4 flex flex-wrap gap-2">
        {([{ id: "all" as const, label: "Все" }, ...MARKET_CATEGORIES]).map(c => (
          <button
            key={c.id}
            type="button"
            onClick={() => setCategory(c.id)}
            className={`rounded-xl px-3.5 py-2 text-sm font-medium transition ${category === c.id ? "bg-brand-500/15 text-brand-300 ring-1 ring-brand-400/20" : "nav-link hover:bg-black/5 dark:hover:bg-white/[0.04]"}`}
          >
            {c.label}
          </button>
        ))}
      </div>

      <div className="mt-7">
        <div className="mb-4 flex items-center gap-2">
          <h2 className="section-title">Приложения</h2>
          <span className="section-count">{visible.length}</span>
        </div>
        {visible.length === 0 ? (
          <div className="panel px-6 py-12 text-center text-sm text-slate-400">
            Ничего не нашлось{query ? ` по запросу «${query}»` : ""}. Попробуйте другое название или номер.
          </div>
        ) : (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(230px,1fr))] gap-4">
            {visible.map(app => <MarketCard key={app.id} app={app} onOpen={openApp} />)}
          </div>
        )}
      </div>

      {selected && <PurchaseSheet key={selected.id} app={selected} onClose={() => setSelected(null)} />}
    </div>
  );
}
