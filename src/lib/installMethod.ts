// Выбор способа установки «USB/Wi-Fi или Wi-Fi сервер» в момент установки.
// Модуль намеренно без зависимий от tauri.ts: он используется и командным
// шлюзом (перехват enqueue-команд), и диалогом выбора.

export type InstallMethod = "usb" | "server";

type Listener = (open: boolean) => void;

let listener: Listener | null = null;
let resolver: ((method: InstallMethod | null) => void) | null = null;

/**
 * Показывает диалог выбора способа установки и ждёт решения пользователя.
 * Второй вызок, пока диалог открыт, не создаёт новый диалог.
 */
export function pickInstallMethod(): Promise<InstallMethod | null> {
  if (resolver) {
    return Promise.resolve(null);
  }
  return new Promise(resolve => {
    resolver = resolve;
    listener?.(true);
  });
}

/** Пользователь сделал выбор (null = закрыл диалог — установка отменена). */
export function resolveInstallMethod(method: InstallMethod | null) {
  const resolve = resolver;
  resolver = null;
  listener?.(false);
  resolve?.(method);
}

export function isInstallMethodPicking() {
  return resolver !== null;
}

export function setInstallMethodListener(l: Listener | null) {
  listener = l;
}
