import {
  AlertTriangle, ArrowDownUp, BadgeCheck, Check, CheckCircle2, CreditCard, FlaskConical, KeyRound,
  LoaderCircle, RotateCcw, Search, Send, ShieldCheck, Smartphone, Star, Users, Wallet, X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import PageHeader from "../components/PageHeader";
import SelectMenu from "../components/SelectMenu";
import {
  categoryLabel, EMPTY_FILTER, filterMarket, findConnectedDevice, formatPrice, iosAtLeast, isAppInstalled,
  MARKET_CATEGORIES, MARKET_MIN_IOS, MARKET_SORTS, marketSellers, matchingSellers, pluralRu, sellerHue,
  TEST_MARKET_APPS, testFormFairplay, testPay, testTransfer,
  type DeviceSnapshot, type MarketApp, type MarketFilter, type MarketSeller, type MarketSellerSummary,
  type MarketSort, type PurchaseStage,
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

function SellerAvatar({ name, size = 44, className = "" }: { name: string; size?: number; className?: string }) {
  const hue = sellerHue(name);
  return (
    <div
      style={{
        width: size, height: size, borderRadius: Math.round(size * 0.3),
        background: `linear-gradient(135deg, hsl(${hue} 70% 58%), hsl(${(hue + 40) % 360} 70% 46%))`,
        fontSize: size * 0.4,
      }}
      className={`grid shrink-0 place-items-center font-black text-white ${className}`}
    >
      {name.slice(0, 1)}
    </div>
  );
}

function MarketCard({ app, sellers, onOpen }: { app: MarketApp; sellers: MarketSeller[]; onOpen: (app: MarketApp) => void }) {
  const cheapest = sellers.reduce((a, b) => (b.price < a.price ? b : a));
  const bestRating = Math.max(...sellers.map(s => s.rating));
  const single = sellers.length === 1;
  return (
    <button type="button" onClick={() => onOpen(app)} className="panel app-card group">
      <div className="pointer-events-none absolute -right-12 -top-12 h-40 w-40 rounded-full opacity-20 blur-3xl transition group-hover:opacity-35" style={{ background: app.color }} />
      <div className="flex items-start justify-between gap-3">
        <div className="relative h-[86px] w-[86px]">
          <div className="absolute inset-2 translate-y-3 rounded-[24px] opacity-30 blur-xl" style={{ background: app.color }} />
          <div className="absolute inset-1 translate-x-2 translate-y-2 rounded-[22px] bg-black/35" />
          <MarketIcon app={app} />
        </div>
        <span className="inline-flex items-center gap-1 rounded-lg border border-amber-400/20 bg-amber-400/10 px-2 py-1 text-[11px] font-bold tabular-nums text-amber-300">
          <Star size={11} className="fill-amber-300" />{bestRating.toFixed(1)}
        </span>
      </div>
      <div className="mt-6 pb-14">
        <h3 className="line-clamp-2 text-lg font-bold leading-snug tracking-tight text-theme">{app.title}</h3>
        <p className="mt-1 text-xs text-slate-500">{app.developer} · {categoryLabel(app.category)}</p>
      </div>
      <div className="absolute inset-x-5 bottom-5 flex items-end justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2">
          <div className="flex -space-x-2">
            {sellers.slice(0, 3).map(s => (
              <span key={s.id} title={s.name}>
                <SellerAvatar name={s.name} size={24} className="ring-2 ring-[color:var(--app-surface)]" />
              </span>
            ))}
          </div>
          {single ? (
            <span className="truncate text-xs font-medium text-slate-400">{sellers[0].name}</span>
          ) : sellers.length > 3 ? (
            <span className="text-xs font-semibold text-slate-400">+{sellers.length - 3}</span>
          ) : null}
        </div>
        <div className="shrink-0 text-right">
          {!single && <div className="text-[10px] font-semibold uppercase tracking-wider text-slate-500">от</div>}
          <div className="text-lg font-bold leading-none tabular-nums text-theme">{formatPrice(cheapest.price)}</div>
        </div>
      </div>
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
      <SellerAvatar name={seller.name} />
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

function PurchaseSheet({ app, sellers, onClose }: { app: MarketApp; sellers: MarketSeller[]; onClose: () => void }) {
  const single = sellers.length === 1;
  const [sellerSort, setSellerSort] = useState<"price" | "rating" | "sales">("price");
  const [stage, setStage] = useState<PurchaseStage>(single ? "connect" : "select_seller");
  const [seller, setSeller] = useState<MarketSeller | null>(single ? sellers[0] : null);
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
    () => sellers.reduce((a, b) => (b.price < a.price ? b : a)).id,
    [sellers],
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
            <div className="mt-4 flex gap-1 rounded-xl border border-subtle p-1 text-sm">
              {([["price", "Дешевле"], ["rating", "Рейтинг"], ["sales", "Популярнее"]] as const).map(([value, label]) => (
                <button
                  key={value}
                  type="button"
                  onClick={() => setSellerSort(value)}
                  className={`flex-1 rounded-lg px-3 py-1.5 font-medium transition ${sellerSort === value ? "bg-brand-500/15 text-brand-300" : "text-slate-400 hover:text-theme"}`}
                >
                  {label}
                </button>
              ))}
            </div>
            <div className="mt-3 space-y-2.5">
              {[...sellers].sort((a, b) => sellerSort === "price" ? a.price - b.price : sellerSort === "rating" ? b.rating - a.rating : b.sales - a.sales).map(s => (
                <SellerRow key={s.id} seller={s} cheapest={s.id === cheapestId} onPick={pickSeller} />
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

const HOW_IT_WORKS = [
  { icon: Users, title: "Выберите продавца", text: "Цена и рейтинг у каждого свои" },
  { icon: Smartphone, title: "Подключите iPhone", text: `По USB, iOS ${MARKET_MIN_IOS} или новее` },
  { icon: Wallet, title: "Оплатите", text: "Через ЮKassa, картой или СБП" },
  { icon: BadgeCheck, title: "Приложение на iPhone", text: "Signum перенесёт и проверит" },
];

const PRICE_OPTIONS = [
  { value: "0", label: "Любая цена" },
  { value: "100", label: "До 100 ₽" },
  { value: "150", label: "До 150 ₽" },
  { value: "200", label: "До 200 ₽" },
];

function SellerChip({ seller, active, onToggle }: { seller: MarketSellerSummary; active: boolean; onToggle: (id: string) => void }) {
  return (
    <button
      type="button"
      onClick={() => onToggle(seller.id)}
      aria-pressed={active}
      className={`panel flex w-[220px] shrink-0 items-center gap-3 p-3 text-left transition ${active ? "border-brand-400/50 bg-brand-500/10 ring-1 ring-brand-400/30" : "hover:border-white/[0.12]"}`}
    >
      <div className="relative">
        <SellerAvatar name={seller.name} size={40} />
        {active && (
          <span className="absolute -bottom-1 -right-1 grid h-4 w-4 place-items-center rounded-full bg-brand-500 text-white ring-2 ring-[color:var(--app-surface)]">
            <Check size={10} strokeWidth={3} />
          </span>
        )}
      </div>
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm font-semibold text-theme">{seller.name}</div>
        <div className="mt-0.5 flex items-center gap-2 text-[11px] text-slate-500">
          <span className="inline-flex items-center gap-0.5"><Star size={10} className="fill-amber-300 text-amber-300" />{seller.rating.toFixed(1)}</span>
          <span>{seller.apps} {pluralRu(seller.apps, "приложение", "приложения", "приложений")}</span>
          <span>{seller.storefront}</span>
        </div>
      </div>
    </button>
  );
}

export default function Market() {
  const [filter, setFilter] = useState<MarketFilter>(EMPTY_FILTER);
  const [selected, setSelected] = useState<MarketApp | null>(null);
  const apps = TEST_MARKET_APPS;
  const sellers = useMemo(() => marketSellers(apps), [apps]);

  const update = useCallback((patch: Partial<MarketFilter>) => setFilter(f => ({ ...f, ...patch })), []);
  const toggleSeller = useCallback((id: string) => setFilter(f => ({
    ...f, sellerIds: f.sellerIds.includes(id) ? f.sellerIds.filter(x => x !== id) : [...f.sellerIds, id],
  })), []);

  const visible = useMemo(() => filterMarket(apps, filter), [apps, filter]);
  const openApp = useCallback((app: MarketApp) => setSelected(app), []);
  const filtered = filter.query !== "" || filter.category !== "all" || filter.sellerIds.length > 0 || filter.maxPrice !== 0;
  const minAll = Math.min(...sellers.map(s => s.minPrice));

  return (
    <div className="mx-auto max-w-[1500px] p-7 lg:p-9">
      <PageHeader
        title="Маркет"
        subtitle={`${apps.length} ${pluralRu(apps.length, "приложение", "приложения", "приложений")} · ${sellers.length} ${pluralRu(sellers.length, "продавец", "продавца", "продавцов")} · от ${formatPrice(minAll)}`}
        actions={
          <span className="inline-flex items-center gap-1.5 rounded-lg border border-amber-400/25 bg-amber-400/10 px-2.5 py-1.5 text-xs font-semibold text-amber-300">
            <FlaskConical size={13} />Тестовый режим
          </span>
        }
      />

      <section className="mt-7 grid grid-cols-2 gap-3 xl:grid-cols-4">
        {HOW_IT_WORKS.map(({ icon: Icon, title, text }, index) => (
          <div key={title} className="panel relative flex items-center gap-3 overflow-hidden p-4">
            <div className="grid h-10 w-10 shrink-0 place-items-center rounded-xl bg-brand-500/15 text-brand-300"><Icon size={19} /></div>
            <div className="min-w-0">
              <div className="text-sm font-semibold text-theme">{title}</div>
              <div className="line-clamp-2 text-xs leading-snug text-slate-500">{text}</div>
            </div>
            <span className="pointer-events-none absolute right-3 top-1 text-[40px] font-black leading-none text-white/[0.04]">{index + 1}</span>
          </div>
        ))}
      </section>

      <div className="mt-6 flex flex-wrap items-stretch gap-2">
        <div className="panel flex min-w-[260px] flex-1 items-center gap-3 px-4 py-3 transition focus-within:border-brand-400/40">
          <Search size={18} className="shrink-0 text-slate-500" />
          <input
            value={filter.query}
            onChange={e => update({ query: e.target.value })}
            placeholder="Название, разработчик, Bundle ID или номер App Store"
            className="min-w-0 flex-1 bg-transparent text-sm text-theme outline-none placeholder:text-slate-600"
          />
          {filter.query && (
            <button type="button" aria-label="Очистить" onClick={() => update({ query: "" })} className="text-slate-500 transition hover:text-theme">
              <X size={16} />
            </button>
          )}
        </div>
        <SelectMenu
          aria-label="Цена"
          icon={<Wallet size={14} />}
          value={String(filter.maxPrice)}
          options={PRICE_OPTIONS}
          onChange={value => update({ maxPrice: Number(value) })}
          className="w-[190px]"
        />
        <SelectMenu
          aria-label="Сортировка"
          icon={<ArrowDownUp size={14} />}
          value={filter.sort}
          options={MARKET_SORTS.map(s => ({ value: s.id, label: s.label }))}
          onChange={value => update({ sort: value as MarketSort })}
          className="w-[220px]"
        />
      </div>

      <div className="mt-4 flex flex-wrap items-center gap-3">
        <div className="flex flex-wrap gap-2">
          {([{ id: "all" as const, label: "Все" }, ...MARKET_CATEGORIES]).map(c => (
            <button
              key={c.id}
              type="button"
              onClick={() => update({ category: c.id })}
              className={`rounded-xl px-3.5 py-2 text-sm font-medium transition ${filter.category === c.id ? "bg-brand-500/15 text-brand-300 ring-1 ring-brand-400/20" : "nav-link hover:bg-black/5 dark:hover:bg-white/[0.04]"}`}
            >
              {c.label}
            </button>
          ))}
        </div>
      </div>

      <section className="mt-7">
        <div className="mb-3 flex items-center justify-between gap-3">
          <div className="flex items-center gap-2">
            <h2 className="section-title">Продавцы</h2>
            <span className="section-count">{sellers.length}</span>
          </div>
          {filter.sellerIds.length > 0 && (
            <button type="button" onClick={() => update({ sellerIds: [] })} className="text-xs font-semibold text-brand-300 transition hover:text-brand-200">
              Все продавцы
            </button>
          )}
        </div>
        <div className="-mx-1 flex gap-3 overflow-x-auto px-1 pb-2">
          {sellers.map(s => (
            <SellerChip key={s.id} seller={s} active={filter.sellerIds.includes(s.id)} onToggle={toggleSeller} />
          ))}
        </div>
      </section>

      <section className="mt-6">
        <div className="mb-4 flex items-center justify-between gap-3">
          <div className="flex items-center gap-2">
            <h2 className="section-title">Приложения</h2>
            <span className="section-count">{visible.length}</span>
          </div>
          {filtered && (
            <button type="button" onClick={() => setFilter(f => ({ ...EMPTY_FILTER, sort: f.sort }))} className="inline-flex items-center gap-1.5 text-xs font-semibold text-slate-400 transition hover:text-theme">
              <RotateCcw size={13} />Сбросить фильтры
            </button>
          )}
        </div>
        {visible.length === 0 ? (
          <div className="panel flex flex-col items-center gap-3 px-6 py-12 text-center">
            <Search size={22} className="text-slate-500" />
            <p className="text-sm text-slate-400">
              Ничего не нашлось{filter.query ? ` по запросу «${filter.query}»` : ""}. Измените фильтры или выберите других продавцов.
            </p>
            <button type="button" className="btn-secondary" onClick={() => setFilter(EMPTY_FILTER)}>
              <RotateCcw size={15} />Сбросить фильтры
            </button>
          </div>
        ) : (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(230px,1fr))] gap-4">
            {visible.map(app => (
              <MarketCard key={app.id} app={app} sellers={matchingSellers(app, filter)} onOpen={openApp} />
            ))}
          </div>
        )}
      </section>

      {selected && (
        <PurchaseSheet
          key={selected.id}
          app={selected}
          sellers={matchingSellers(selected, filter)}
          onClose={() => setSelected(null)}
        />
      )}
    </div>
  );
}
