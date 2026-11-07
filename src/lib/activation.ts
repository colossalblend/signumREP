import { command } from "./tauri";
import type { AppActivation, AppActivationItem, TbActivationResponse } from "../types";

const PENDING_KEY = "signum-post-install-activation";

export interface PendingPostInstallActivation {
  jobId: string;
  appTitle: string;
                                                        
  activation: AppActivation;
}

export interface HydratedActivationItem extends AppActivationItem {
                                                      
  expires_at?: number;
}

                                                                  
export function resolveAppActivation(fromApi?: AppActivation | null): AppActivation | null {
  if (!fromApi || fromApi.enabled === false) return null;
  const items = (fromApi.items ?? []).filter(
    item => activationPayload(item).length > 0 || !!(item.tb_id || "").trim(),
  );
  if (!items.length) return null;
  return { ...fromApi, items };
}

export function activationPayload(item: AppActivationItem): string {
  return (item.qr_payload || item.url || "").trim();
}

                                                                        
export function matchActivationItem(
  activation: AppActivation,
  match: { app_store_id?: string | null; bundle_id?: string | null; title?: string | null },
): AppActivationItem | null {
  const storeId = match.app_store_id?.trim() || "";
  const bundle = match.bundle_id?.trim() || "";
  const title = match.title?.trim().toLowerCase() || "";

  if (storeId) {
    const byStore = activation.items.find(i => (i.app_store_id || "").trim() === storeId);
    if (byStore) return byStore;
  }
  if (bundle && bundle !== "NONE") {
    const byBundle = activation.items.find(i => (i.bundle_id || "").trim() === bundle);
    if (byBundle) return byBundle;
  }
  if (title) {
    const byTitle = activation.items.find(i => i.title.trim().toLowerCase() === title);
    if (byTitle) return byTitle;
  }
  return null;
}

                                                                  
export function activationForInstalledVersion(
  full: AppActivation,
  match: { app_store_id?: string | null; bundle_id?: string | null; title?: string | null },
): AppActivation | null {
  const item = matchActivationItem(full, match);
  if (!item) return null;
  return {
    enabled: true,
    provider: full.provider,
    title: full.title || "Активация после установки",
    message:
      item.hint?.trim() ||
      full.message ||
      "Для входа в банк нужно активировать установленное приложение.",
    items: [item],
  };
}

   
                                        
                                                                     
   
export async function fetchTbActivationPayload(
  tbId: string,
  force = false,
): Promise<{ payload: string; expiresIn: number; hint?: string }> {
  const result = await command<TbActivationResponse>("fetch_tb_activation", {
    id: tbId,
    force,
  });
  const payload = (result.qr_payload || "").trim();
  if (!payload) {
    throw new Error(result.error || "Не удалось получить QR-код");
  }
  const expiresIn = Math.max(0, Number(result.expires_in ?? result.ttl_seconds ?? 600));
  return {
    payload,
    expiresIn,
    hint: result.hint || undefined,
  };
}

   
                                                         
                                                            
                                                      
   
export async function hydrateActivation(
  activation: AppActivation,
  force = false,
): Promise<AppActivation> {
  const items = await Promise.all(
    activation.items.map(async (item): Promise<HydratedActivationItem> => {
      const tbId = (item.tb_id || "").trim();
      if (!tbId) return item;
      try {
        const { payload, expiresIn, hint } = await fetchTbActivationPayload(tbId, force);
        return {
          ...item,
          qr_payload: payload,
          mode: item.mode || "deeplink",
          hint:
            item.hint ||
            hint ||
            "Отсканируйте QR камерой iPhone. Код действует 10 минут, затем обновляется.",
          expires_at: Date.now() + expiresIn * 1000,
        };
      } catch (error) {
        return {
          ...item,
          qr_payload: force ? "" : item.qr_payload || "",
          hint:
            error instanceof Error
              ? `Не удалось сгенерировать QR: ${error.message}`
              : "Не удалось сгенерировать QR активации.",
        };
      }
    }),
  );
  return { ...activation, items };
}

export function storePendingPostInstallActivation(payload: PendingPostInstallActivation): void {
  try {
    sessionStorage.setItem(PENDING_KEY, JSON.stringify(payload));
  } catch {
                                  
  }
}

export function takePendingPostInstallActivation(jobId: string): PendingPostInstallActivation | null {
  try {
    const raw = sessionStorage.getItem(PENDING_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as PendingPostInstallActivation;
    if (!parsed?.jobId || parsed.jobId !== jobId || !parsed.activation?.items?.length) {
      return null;
    }
    sessionStorage.removeItem(PENDING_KEY);
    return parsed;
  } catch {
    return null;
  }
}
