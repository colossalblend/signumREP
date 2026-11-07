import { listen } from "@tauri-apps/api/event";
import { LoaderCircle, ShieldCheck } from "lucide-react";
import { FormEvent, useEffect, useState } from "react";
import { command, errorMessage } from "../lib/tauri";

   
                                                      
   
export default function Signing2faGate() {
  const [open, setOpen] = useState(false);
  const [message, setMessage] = useState("");
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [resending, setResending] = useState(false);
  const [error, setError] = useState("");
  const [hint, setHint] = useState("");

  useEffect(() => {
    const pending = listen<{ message?: string; sms?: boolean }>("signing-2fa-required", event => {
      setMessage(
        event.payload?.message?.trim() ||
          "Введите 6-значный код с iPhone.",
      );
      setCode("");
      setError("");
      setHint("");
      setOpen(true);
      setResending(false);
      setBusy(false);
    });
    // Отписываемся через сам промис: если компонент снимут раньше, чем listen
    // успеет ответить, подписка всё равно будет снята.
    return () => {
      void pending.then(unlisten => unlisten());
    };
  }, []);

  async function submit(e?: FormEvent) {
    e?.preventDefault();
    const digits = code.replace(/\D/g, "").slice(0, 6);
    if (digits.length < 6) {
      setError("Введите 6 цифр");
      return;
    }
    setBusy(true);
    setError("");
    try {
      await command("submit_signing_2fa", { code: digits, action: "submit" });
      setOpen(false);
      setCode("");
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  async function resend() {
    setResending(true);
    setError("");
    setHint("");
    try {
                                                                                        
      await command("submit_signing_2fa", { code: null, action: "resend" });
      setHint("Запрос на повторную отправку кода отправлен.");
      setCode("");
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setResending(false);
    }
  }

  async function cancel() {
    setOpen(false);
    void command("submit_signing_2fa", { code: "", action: "cancel" }).catch(() => undefined);
  }

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-[200] flex items-center justify-center bg-black/55 p-4 backdrop-blur-sm">
      <div className="panel w-full max-w-md p-6 shadow-2xl">
        <div className="flex items-center gap-2 font-bold">
          <ShieldCheck size={20} className="text-brand-300" />
          Код 2FA для подписи
        </div>
        <p className="mt-3 text-sm leading-relaxed text-slate-400">{message}</p>
        <form className="mt-5 space-y-3" onSubmit={e => void submit(e)}>
          <input
            autoFocus
            inputMode="numeric"
            autoComplete="one-time-code"
            className="w-full rounded-xl border border-white/10 bg-black/20 px-4 py-3 text-center text-2xl tracking-[0.4em]"
            placeholder="••••••"
            value={code}
            maxLength={6}
            onChange={e => setCode(e.target.value.replace(/\D/g, "").slice(0, 6))}
          />
          {error && <p className="text-sm text-amber-300">{error}</p>}
          {hint && !error && <p className="text-sm text-emerald-300/90">{hint}</p>}
          <div className="flex flex-wrap gap-2">
            <button type="submit" className="btn-primary flex-1" disabled={busy || resending || code.length < 6}>
              {busy ? <LoaderCircle size={16} className="animate-spin" /> : null}
              Подтвердить
            </button>
            <button
              type="button"
              className="btn-secondary"
              disabled={busy || resending}
              onClick={() => void resend()}
            >
              {resending ? <LoaderCircle size={16} className="animate-spin" /> : null}
              Отправить код ещё раз
            </button>
            <button type="button" className="btn-secondary" disabled={busy || resending} onClick={() => void cancel()}>
              Отмена
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
