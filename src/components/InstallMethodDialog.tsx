import { invoke } from "@tauri-apps/api/core";
import { Cable, Check, LoaderCircle, QrCode } from "lucide-react";
import { useEffect, useState } from "react";
import { resolveInstallMethod, setInstallMethodListener } from "../lib/installMethod";
import type { DeviceInfo } from "../types";

function connectionLabel(connection?: string): string {
  switch ((connection || "usb").toLowerCase()) {
    case "wifi":
      return "Wi-Fi";
    case "both":
      return "USB + Wi-Fi";
    default:
      return "USB";
  }
}

/**
 * Диалог выбора способа установки на iPhone. Открывается перед постановкой
 * установки в очередь: сразу же ищет устройство по USB и Wi-Fi (как в Xcode)
 * и подсвечивает подходящий способ — телефон найден → USB/Wi-Fi, не найден →
 * Wi-Fi сервер. Способ всегда можно переключить вручную.
 */
export default function InstallMethodDialog() {
  const [open, setOpen] = useState(false);
  const [devices, setDevices] = useState<DeviceInfo[] | null>(null);

  useEffect(() => {
    setInstallMethodListener(o => setOpen(o));
    return () => setInstallMethodListener(null);
  }, []);

  useEffect(() => {
    if (!open) {
      setDevices(null);
      return;
    }
    void invoke<DeviceInfo[]>("list_connected_devices")
      .then(d => setDevices(d))
      .catch(() => setDevices([]));
  }, [open]);

  if (!open) return null;

  const detecting = devices === null;
  const device = devices?.[0];
  const found = (devices?.length ?? 0) > 0;
  // Телефон найден → сразу предлагаем кабель/сеть, нет — Wi-Fi сервер.
  const recommended: "usb" | "server" = found ? "usb" : "server";

  function pick(method: "usb" | "server") {
    resolveInstallMethod(method);
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-6 backdrop-blur-sm">
      <div className="w-full max-w-md rounded-2xl border border-white/10 bg-[#141d30] p-6 shadow-2xl">
        <div className="text-base font-semibold text-slate-100">Установка на iPhone</div>
        <div className="mt-1 text-xs leading-4 text-slate-400">
          {detecting
            ? "Ищем iPhone по USB и Wi-Fi…"
            : found
              ? "iPhone найден — выберите, как установить приложение."
              : "iPhone не найден — доступен Wi-Fi сервер (QR-код)."}
        </div>

        <div className="mt-4 grid gap-2">
          <button
            type="button"
            onClick={() => pick("usb")}
            className={`w-full rounded-xl border p-4 text-left transition ${
              recommended === "usb"
                ? "border-brand-400/40 bg-brand-500/10"
                : "border-white/[0.08] bg-black/10 hover:border-white/20"
            }`}
          >
            <div className="flex items-center gap-2">
              <Cable size={16} className="text-brand-300" />
              <span className="text-sm font-semibold text-slate-100">USB / Wi-Fi</span>
              {recommended === "usb" && !detecting && (
                <span className="ml-auto flex items-center gap-1 rounded-full bg-emerald-400/10 px-2 py-0.5 text-[10px] font-semibold text-emerald-300">
                  <Check size={11} /> {found ? "iPhone найден" : "Рекомендуется"}
                </span>
              )}
            </div>
            <div className="mt-1.5 flex items-center gap-1.5 text-[11px] leading-4 text-slate-400">
              {detecting ? (
                <LoaderCircle size={11} className="animate-spin" />
              ) : found ? (
                <>
                  {device?.device_name || "iPhone"}
                  {device?.connection ? ` · ${connectionLabel(device.connection)}` : ""}
                </>
              ) : (
                "iPhone не найден — подключите по кабелю"
              )}
            </div>
          </button>

          <button
            type="button"
            onClick={() => pick("server")}
            className={`w-full rounded-xl border p-4 text-left transition ${
              recommended === "server"
                ? "border-brand-400/40 bg-brand-500/10"
                : "border-white/[0.08] bg-black/10 hover:border-white/20"
            }`}
          >
            <div className="flex items-center gap-2">
              <QrCode size={16} className="text-brand-300" />
              <span className="text-sm font-semibold text-slate-100">Wi-Fi сервер</span>
              {recommended === "server" && !detecting && (
                <span className="ml-auto flex items-center gap-1 rounded-full bg-emerald-400/10 px-2 py-0.5 text-[10px] font-semibold text-emerald-300">
                  <Check size={11} /> Кабель не нужен
                </span>
              )}
            </div>
            <div className="mt-1.5 text-[11px] leading-4 text-slate-400">
              QR-код · iPhone скачает и установит приложение сам
            </div>
          </button>
        </div>

        <button
          type="button"
          className="mt-4 w-full rounded-lg py-2 text-xs text-slate-500 transition hover:text-slate-300"
          onClick={() => resolveInstallMethod(null)}
        >
          Отмена
        </button>
      </div>
    </div>
  );
}
