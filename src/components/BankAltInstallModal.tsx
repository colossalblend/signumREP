import {
  Check, ChevronDown, ChevronLeft, ChevronRight, Copy, LoaderCircle, Smartphone, X,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  bankDisplayName, bankIconUrl, bankMinIos, iosMeetsMin,
} from "../lib/bankAlt";
import { command, errorMessage } from "../lib/tauri";
import type { BankKey } from "../types";

                                                                                      
const MANUAL_IMG = {
  copy: "https://manual.sovcombank.ru/_next/static/media/step3.cc5e3bcd.webp",
  settings: "https://manual.sovcombank.ru/_next/static/media/step1.0f206336.webp",
  exitContent: "https://manual.sovcombank.ru/_next/static/media/step2.ae5cd83a.webp",
  contentPurchases: "https://manual.sovcombank.ru/_next/static/media/step3.54efa08e.webp",
  otherOptions: "https://manual.sovcombank.ru/_next/static/media/step4.08e872de.webp",
  appStore: "https://manual.sovcombank.ru/_next/static/media/accessIsOpen.e754a3c3.png",
} as const;

const MANUAL_STEPS: Array<{
  n: number;
  title: string;
  text: string;
  img: string;
  cap?: string;
}> = [
  {
    n: 1,
    title: "Скопируйте e-mail и пароль",
    text: "Скопируйте данные кнопками выше — они нужны для входа. Действуют около 30 минут.",
    img: MANUAL_IMG.copy,
  },
  {
    n: 2,
    title: "Откройте «Настройки» и перейдите в профиль",
    text: "Нажмите на своё имя вверху и выберите «Контент и покупки».",
    img: MANUAL_IMG.settings,
    cap: "Не выходите из своего аккаунта Apple в настройках телефона",
  },
  {
    n: 3,
    title: "Выйдите из раздела «Контент и покупки»",
    text: "Выберите раздел → «Выйти» → подтвердите выход.",
    img: MANUAL_IMG.exitContent,
    cap: "Позже при входе в свой Apple ID пароль часто вводить не нужно",
  },
  {
    n: 4,
    title: "Настройте раздел «Контент и покупки»",
    text: "Снова откройте раздел. Если появится окно — «Не Имя Фамилия?» → «Продолжить» и войдите скопированными e-mail и паролем.",
    img: MANUAL_IMG.contentPurchases,
  },
  {
    n: 5,
    title: "После входа: «Другие возможности»",
    text: "Выберите «Другие возможности», затем нажмите «Не улучшать».",
    img: MANUAL_IMG.otherOptions,
  },
  {
    n: 6,
    title: "Установите любое бесплатное приложение",
    text: "Откройте App Store, найдите любое бесплатное приложение, нажмите «Загрузить» и подтвердите. После начала загрузки активация завершена — это приложение можно удалить.",
    img: MANUAL_IMG.appStore,
    cap: "Если приложение банка вылетает — попробуйте снова: загрузите другое бесплатное приложение из App Store (потом его можно удалить).",
  },
];

type Step = "loading" | "phone" | "code" | "creds";

type Props = {
  bank: BankKey;
  onClose: () => void;
};

function qrImageUrl(data: string, size = 200): string {
  return `https://api.qrserver.com/v1/create-qr-code/?size=${size}x${size}&margin=8&data=${encodeURIComponent(data)}`;
}

function formatRuPhoneInput(raw: string): string {
  let digits = raw.replace(/\D/g, "");
  if (!digits) return "";
  if (digits.startsWith("8")) digits = `7${digits.slice(1)}`;
  if (digits.length <= 10 && digits.startsWith("9")) digits = `7${digits}`;
  if (!digits.startsWith("7")) digits = `7${digits}`;
  digits = digits.slice(0, 11);
  const rest = digits.slice(1);
  let out = "+7";
  if (rest.length > 0) out += ` ${rest.slice(0, 3)}`;
  if (rest.length > 3) out += ` ${rest.slice(3, 6)}`;
  if (rest.length > 6) out += ` ${rest.slice(6, 10)}`;
  return out;
}

function phoneDigitsForApi(formatted: string): string {
  let d = formatted.replace(/\D/g, "");
  if (d.startsWith("8") && d.length === 11) d = `7${d.slice(1)}`;
  if (d.length === 10 && d.startsWith("9")) d = `7${d}`;
  return d;
}

type ConfirmResult = {
  job_id: string;
  login: string;
  password: string;
  page_url?: string | null;
  page_ready?: boolean;
  remaining_secs?: number;
  expires_in_label?: string;
  from_cache?: boolean;
};

async function stopQrSite() {
  try {
    await command("bank_alt_stop_page");
  } catch {
                
  }
}

export default function BankAltInstallModal({ bank, onClose }: Props) {
  const navigate = useNavigate();
  const bankName = bankDisplayName(bank);
  const bankIcon = bankIconUrl(bank);
  const minIos = bankMinIos(bank);
  const [step, setStep] = useState<Step>("loading");
  const [phone, setPhone] = useState("");
  const [code, setCode] = useState("");
  const [sessionId, setSessionId] = useState("");
  const [phoneDisplay, setPhoneDisplay] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [login, setLogin] = useState("");
  const [password, setPassword] = useState("");
  const [pageUrl, setPageUrl] = useState<string | null>(null);
  const [qrFailed, setQrFailed] = useState(false);
  const [copied, setCopied] = useState<"login" | "password" | null>(null);
  const [expiresLabel, setExpiresLabel] = useState("");
  const [remainingSecs, setRemainingSecs] = useState(0);
  const [fromCache, setFromCache] = useState(false);
                                                                             
  const [showManual, setShowManual] = useState(false);
  const [deviceIos, setDeviceIos] = useState<string | null>(null);
  const [deviceName, setDeviceName] = useState<string | null>(null);

  function applyConfirm(res: ConfirmResult) {
    setLogin(res.login);
    setPassword(res.password);
    setPageUrl(res.page_ready && res.page_url ? res.page_url : null);
    setQrFailed(false);
    setExpiresLabel(res.expires_in_label || "ещё 30 мин");
    setRemainingSecs(res.remaining_secs ?? 30 * 60);
    setFromCache(!!res.from_cache);
                                              
    setShowManual(!(res.page_ready && res.page_url));
    setStep("creds");
  }

  async function handleClose() {
    await stopQrSite();
    onClose();
  }

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const device = await command<{
          product_version?: string;
          device_name?: string;
          name?: string;
        } | null>("get_device_info");
        if (!cancelled && device) {
          setDeviceIos(device.product_version?.trim() || null);
          setDeviceName((device.device_name || device.name || "").trim() || null);
        }
      } catch {
        if (!cancelled) {
          setDeviceIos(null);
          setDeviceName(null);
        }
      }

      try {
        const cached = await command<{
          active: boolean;
          login: string;
          password: string;
          remaining_secs: number;
          expires_in_label: string;
        } | null>("bank_alt_peek_cache", { input: { bank } });
        if (cancelled) return;
        if (cached?.active) {
          setLogin(cached.login);
          setPassword(cached.password);
          setExpiresLabel(cached.expires_in_label);
          setRemainingSecs(cached.remaining_secs);
          setFromCache(true);
          setStep("creds");
          setBusy(true);
          try {
            const res = await command<ConfirmResult>("bank_alt_resume_from_cache", {
              input: { bank },
            });
            if (!cancelled) applyConfirm(res);
          } catch (e) {
            if (!cancelled) {
              setError(errorMessage(e));
              setShowManual(true);
              setStep("creds");
            }
          } finally {
            if (!cancelled) setBusy(false);
          }
        } else {
          setStep("phone");
        }
      } catch {
        if (!cancelled) setStep("phone");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [bank]);

  useEffect(() => {
    if (step !== "creds" || remainingSecs <= 0) return;
    const t = window.setInterval(() => {
      setRemainingSecs(prev => {
        const next = prev - 1;
        if (next <= 0) {
          setExpiresLabel("истекла");
          void stopQrSite();
          return 0;
        }
        const m = Math.floor(next / 60);
        const s = next % 60;
        setExpiresLabel(m >= 1 ? `ещё ${m} мин` : `ещё ${s} сек`);
        return next;
      });
    }, 1000);
    return () => window.clearInterval(t);
  }, [step, remainingSecs > 0]);

  async function sendSms() {
    setBusy(true);
    setError("");
    try {
      const res = await command<{
        session_id: string;
        phone_display: string;
      }>("bank_alt_send_sms", {
        input: { bank, phone: phoneDigitsForApi(phone) },
      });
      setSessionId(res.session_id);
      setPhoneDisplay(res.phone_display);
      setStep("code");
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function confirmAndInstall() {
    setBusy(true);
    setError("");
    try {
      const res = await command<ConfirmResult>("bank_alt_confirm_and_install", {
        input: { session_id: sessionId, code },
      });
      applyConfirm(res);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function resumeInstall() {
    setBusy(true);
    setError("");
    try {
      const res = await command<ConfirmResult>("bank_alt_resume_from_cache", {
        input: { bank },
      });
      applyConfirm(res);
    } catch (e) {
      setError(errorMessage(e));
      if (String(errorMessage(e)).toLowerCase().includes("истек")) {
        setStep("phone");
        setLogin("");
        setPassword("");
      }
    } finally {
      setBusy(false);
    }
  }

  async function forceNewAuth() {
    setBusy(true);
    setError("");
    await stopQrSite();
    try {
      await command("bank_alt_clear_cache", { input: { bank } });
    } catch {
                  
    }
    setLogin("");
    setPassword("");
    setPageUrl(null);
    setCode("");
    setSessionId("");
    setFromCache(false);
    setExpiresLabel("");
    setRemainingSecs(0);
    setShowManual(false);
    setStep("phone");
    setBusy(false);
  }

  async function copyText(kind: "login" | "password", value: string) {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(kind);
      window.setTimeout(() => setCopied(null), 1500);
    } catch {
                  
    }
  }

  const phoneReady = phoneDigitsForApi(phone).length === 11;
  const expired = step === "creds" && remainingSecs <= 0 && fromCache;
  const hasQr = !expired && !!pageUrl && !qrFailed;
  const iosOk = iosMeetsMin(deviceIos, minIos);
  const iosBlocked = iosOk === false;

  return (
    <div
      className="fixed inset-0 z-[70] grid place-items-center bg-black/70 p-6 backdrop-blur-md"
      onMouseDown={() => {
        if (!busy && step !== "creds") void handleClose();
      }}
    >
      <div
        className="panel max-h-[92vh] w-full max-w-lg overflow-y-auto p-6"
        onMouseDown={e => e.stopPropagation()}
      >
        <div className="flex items-start justify-between gap-3">
          <div className="flex items-start gap-3">
            <img
              src={bankIcon}
              alt={bankName}
              className="h-12 w-12 shrink-0 rounded-2xl object-cover shadow-glow ring-1 ring-white/10"
              referrerPolicy="no-referrer"
            />
            <div>
              <h2 className="text-lg font-bold">Альтернативная установка</h2>
              <p className="mt-0.5 text-sm text-slate-400">{bankName}</p>
              <p className="mt-1 text-xs text-slate-500">
                {deviceIos
                  ? `${deviceName ? `${deviceName} · ` : ""}iOS ${deviceIos}`
                  : "iPhone не подключён · версия iOS неизвестна"}
                {" · "}нужна iOS {minIos}+
              </p>
            </div>
          </div>
          {!busy && (
            <button
              type="button"
              className="rounded-lg p-2 text-slate-500 hover:bg-white/5 hover:text-white"
              onClick={() => void handleClose()}
              title="Закрыть"
            >
              <X size={18} />
            </button>
          )}
        </div>

        {iosBlocked && (
          <div className="mt-4 rounded-xl border border-rose-400/30 bg-rose-400/10 p-3.5 text-sm leading-6 text-rose-50/95">
            <strong className="text-rose-100">
              {bankName} не запустится на iOS {deviceIos}
            </strong>
            <p className="mt-1 text-rose-100/85">
              Нужна iOS {minIos}+.
              {bank === "sovcombank" && iosMeetsMin(deviceIos, "16.0")
                ? " Можно поставить Рокетбанк — он работает с iOS 16.0."
                : bank === "sovcombank"
                  ? " Рокетбанк тоже с iOS 16.0 — обновите iPhone."
                  : " Обновите iPhone или выберите другой банк."}
            </p>
          </div>
        )}

        {!iosBlocked && deviceIos && iosOk && (
          <div className="mt-4 rounded-xl border border-emerald-400/20 bg-emerald-400/10 px-3.5 py-2.5 text-xs leading-5 text-emerald-100/90">
            iPhone iOS {deviceIos} — подходит для {bankName} (нужна {minIos}+)
          </div>
        )}

        {step === "loading" && (
          <div className="mt-8 flex flex-col items-center gap-3 py-6">
            <LoaderCircle className="animate-spin text-brand-300" size={28} />
            <p className="text-sm text-slate-400">Проверяем сохранённую учётку…</p>
          </div>
        )}

        {step === "phone" && (
          <div className="mt-5 space-y-3">
            <p className="text-sm leading-6 text-slate-400">
              Введите номер — получите SMS-код. Учётка действует 30 минут: при закрытии окна
              телефон и код вводить снова не нужно.
            </p>
            <label className="block">
              <span className="mb-1.5 block text-xs text-slate-500">Номер телефона</span>
              <input
                className="field tracking-wide"
                type="tel"
                inputMode="tel"
                placeholder="+7 902 220 8917"
                value={phone}
                onChange={e => setPhone(formatRuPhoneInput(e.target.value))}
                onKeyDown={e => {
                  if (e.key === "Enter" && phoneReady) void sendSms();
                }}
                autoFocus
              />
            </label>
            <button
              type="button"
              className="btn-primary w-full"
              disabled={busy || !phoneReady || iosBlocked}
              onClick={() => void sendSms()}
            >
              {busy ? <LoaderCircle size={16} className="animate-spin" /> : <Smartphone size={16} />}
              {busy ? "Отправляем SMS…" : iosBlocked ? `Нужна iOS ${minIos}+` : "Отправить SMS"}
            </button>
          </div>
        )}

        {step === "code" && (
          <div className="mt-5 space-y-3">
            <p className="text-sm text-slate-400">
              Код отправлен на{" "}
              <span className="font-semibold text-slate-200">{phoneDisplay || phone}</span>
            </p>
            <label className="block">
              <span className="mb-1.5 block text-xs text-slate-500">Код из SMS</span>
              <input
                className="field tracking-[0.3em]"
                inputMode="numeric"
                placeholder="••••"
                value={code}
                onChange={e => setCode(e.target.value.replace(/\D/g, "").slice(0, 8))}
                onKeyDown={e => {
                  if (e.key === "Enter" && code.length >= 4 && !iosBlocked) void confirmAndInstall();
                }}
                autoFocus
              />
            </label>
            <button
              type="button"
              className="btn-primary w-full"
              disabled={busy || code.length < 4 || iosBlocked}
              onClick={() => void confirmAndInstall()}
            >
              {busy ? <LoaderCircle size={16} className="animate-spin" /> : null}
              {busy ? "Получаем данные…" : "Подтвердить"}
            </button>
            <button
              type="button"
              className="btn-secondary w-full text-xs"
              disabled={busy}
              onClick={() => { setStep("phone"); setCode(""); setError(""); }}
            >
              Другой номер
            </button>
          </div>
        )}

        {step === "creds" && (
          <div className="mt-5 space-y-4">
            {expired ? (
              <div className="rounded-xl border border-amber-400/25 bg-amber-400/[0.08] p-4 text-sm leading-6 text-amber-100/90">
                Учётка банка истекла (30 мин). Нужен новый телефон и SMS-код.
              </div>
            ) : (
              <div className="rounded-xl border border-emerald-400/20 bg-emerald-400/10 p-3.5 text-sm leading-6 text-emerald-100/90">
                {fromCache
                  ? "Сохранённая учётка — SMS не нужен. "
                  : "Приложение скачивается и ставится на iPhone. "}
                Данные действительны: <strong>{expiresLabel || "ещё 30 мин"}</strong>
              </div>
            )}

            {                      }
            {!expired && hasQr && (
              <div className="rounded-2xl border border-white/[0.08] bg-black/25 p-5 text-center">
                <p className="text-base font-semibold text-slate-100">Сканируйте QR с iPhone</p>
                <p className="mt-1.5 text-sm leading-5 text-slate-400">
                  Откроется страница: копирование логина/пароля и прогресс установки.
                  iPhone и ПК должны быть в одной Wi‑Fi сети.
                </p>
                <div className="mx-auto mt-4 grid w-[220px] place-items-center rounded-2xl bg-white p-3 shadow-lg">
                  <img
                    src={qrImageUrl(pageUrl!, 200)}
                    alt="QR для iPhone"
                    width={200}
                    height={200}
                    className="rounded-xl"
                    onError={() => {
                      setQrFailed(true);
                      setShowManual(true);
                    }}
                  />
                </div>
                <p className="mt-3 break-all text-[11px] text-slate-600">{pageUrl}</p>
              </div>
            )}

            {!expired && !hasQr && !showManual && (
              <div className="rounded-xl border border-white/[0.06] bg-black/20 p-4 text-sm text-slate-400">
                QR недоступен — откройте данные Apple ID и инструкцию кнопкой ниже.
              </div>
            )}

            {                     }
            {!expired && (
              <button
                type="button"
                className="btn-secondary w-full justify-between"
                onClick={() => setShowManual(v => !v)}
              >
                <span>{showManual ? "Скрыть данные и инструкцию" : "Сайт не открывается"}</span>
                <ChevronDown
                  size={16}
                  className={`transition ${showManual ? "rotate-180" : ""}`}
                />
              </button>
            )}

            {                                             }
            {!expired && showManual && (
              <div className="space-y-4">
                <CredRow
                  label="Логин (e-mail)"
                  value={login}
                  copied={copied === "login"}
                  onCopy={() => void copyText("login", login)}
                />
                <CredRow
                  label="Пароль"
                  value={password}
                  copied={copied === "password"}
                  onCopy={() => void copyText("password", password)}
                />
                <BankInstallGuideIllustrated />
              </div>
            )}

            {expired ? (
              <button type="button" className="btn-primary w-full" onClick={() => void forceNewAuth()}>
                Ввести телефон снова
              </button>
            ) : (
              <>
                <button
                  type="button"
                  className="btn-primary w-full"
                  disabled={busy}
                  onClick={() => {
                    void stopQrSite();
                    onClose();
                    navigate("/installs");
                  }}
                >
                  К установкам
                </button>
                <button
                  type="button"
                  className="btn-secondary w-full text-xs"
                  disabled={busy}
                  onClick={() => void resumeInstall()}
                >
                  {busy ? <LoaderCircle size={14} className="animate-spin" /> : null}
                  Повторить установку / обновить QR
                </button>
                <button
                  type="button"
                  className="btn-secondary w-full text-xs text-slate-500"
                  disabled={busy}
                  onClick={() => void forceNewAuth()}
                >
                  Новая авторизация (другой номер)
                </button>
                <button
                  type="button"
                  className="btn-secondary w-full text-xs"
                  onClick={() => void handleClose()}
                >
                  Закрыть
                </button>
              </>
            )}
          </div>
        )}

        {error && (
          <div className="mt-4 rounded-xl border border-amber-400/25 bg-amber-400/[0.08] p-3 text-sm leading-6 text-amber-100/90">
            {error}
          </div>
        )}
      </div>
    </div>
  );
}

function CredRow({
  label,
  value,
  copied,
  onCopy,
}: {
  label: string;
  value: string;
  copied: boolean;
  onCopy: () => void;
}) {
  return (
    <div className="rounded-xl border border-white/[0.08] bg-black/20 p-3.5">
      <div className="mb-1.5 text-xs font-medium uppercase tracking-wide text-slate-500">
        {label}
      </div>
      <div className="flex items-center gap-2">
        <code className="min-w-0 flex-1 break-all text-[15px] font-semibold text-slate-100">
          {value || "—"}
        </code>
        <button
          type="button"
          className="btn-secondary shrink-0 px-3 py-2 text-xs"
          onClick={onCopy}
          title="Копировать"
        >
          {copied ? <Check size={15} className="text-emerald-400" /> : <Copy size={15} />}
        </button>
      </div>
    </div>
  );
}

                                                                        
function BankInstallGuideIllustrated() {
  const [idx, setIdx] = useState(0);
  const touchX = useRef<number | null>(null);
  const total = MANUAL_STEPS.length;
  const s = MANUAL_STEPS[idx];
  const isFirst = idx === 0;
  const isLast = idx === total - 1;

  function go(delta: number) {
    setIdx(i => Math.max(0, Math.min(total - 1, i + delta)));
  }

  return (
    <div className="rounded-2xl border border-amber-400/30 bg-gradient-to-b from-amber-400/[0.1] to-transparent p-4">
      <div className="mb-3 flex items-start justify-between gap-3">
        <div>
          <h3 className="text-base font-bold text-amber-50">Пошаговая инструкция</h3>
          <p className="mt-1 text-sm leading-5 text-amber-100/80">
            Листайте шаги — как войти через «Контент и покупки»
          </p>
        </div>
        <div className="shrink-0 rounded-full bg-amber-400/15 px-2.5 py-1 text-xs font-semibold text-amber-100">
          {idx + 1} / {total}
        </div>
      </div>

      <div
        className="select-none"
        onTouchStart={e => {
          touchX.current = e.changedTouches[0]?.clientX ?? null;
        }}
        onTouchEnd={e => {
          const start = touchX.current;
          touchX.current = null;
          if (start == null) return;
          const dx = (e.changedTouches[0]?.clientX ?? start) - start;
          if (Math.abs(dx) < 40) return;
          go(dx < 0 ? 1 : -1);
        }}
      >
        <div className="mb-3 flex items-start gap-2">
          <span className="mt-0.5 grid h-7 w-7 shrink-0 place-items-center rounded-full bg-amber-400/25 text-xs font-bold text-amber-100">
            {s.n}
          </span>
          <div className="min-w-0 flex-1">
            <div className="text-sm font-semibold leading-snug text-slate-100">{s.title}</div>
            <p className="mt-1 text-[13px] leading-5 text-slate-300">{s.text}</p>
          </div>
        </div>

        <img
          key={s.img}
          src={s.img}
          alt={s.title}
          loading="eager"
          referrerPolicy="no-referrer"
          className="mx-auto w-full max-w-sm rounded-xl border border-white/10 bg-black/40 object-cover"
        />
        {s.cap ? (
          <p className="mt-2 text-center text-[11px] text-slate-500">{s.cap}</p>
        ) : null}
      </div>

      <div className="mt-4 flex items-center gap-2">
        <button
          type="button"
          className="btn-secondary h-10 w-10 shrink-0 px-0"
          disabled={isFirst}
          onClick={() => go(-1)}
          title="Назад"
          aria-label="Предыдущий шаг"
        >
          <ChevronLeft size={18} />
        </button>
        <div className="flex min-w-0 flex-1 items-center justify-center gap-1.5">
          {MANUAL_STEPS.map((_, i) => (
            <button
              key={i}
              type="button"
              aria-label={`Шаг ${i + 1}`}
              onClick={() => setIdx(i)}
              className={`h-2 rounded-full transition-all ${
                i === idx
                  ? "w-5 bg-amber-300"
                  : "w-2 bg-white/20 hover:bg-white/35"
              }`}
            />
          ))}
        </div>
        <button
          type="button"
          className="btn-primary h-10 min-w-[7.5rem] shrink-0 px-3 text-sm"
          onClick={() => (isLast ? setIdx(0) : go(1))}
        >
          {isLast ? (
            "Сначала"
          ) : (
            <>
              Далее
              <ChevronRight size={16} />
            </>
          )}
        </button>
      </div>

      <div className="mt-4 rounded-xl border border-white/10 bg-black/30 px-3.5 py-3 text-[13px] leading-5 text-slate-300">
        Не выходите из Apple ID полностью в «Настройках». Нужен только вход через
        «Контент и покупки». После активации можно снова выбрать свой аккаунт.
      </div>
    </div>
  );
}
