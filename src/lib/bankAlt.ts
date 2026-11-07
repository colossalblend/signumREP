import type { BankKey, CatalogApp } from "../types";
import { normalizeCatalogSearch } from "./catalogSearch";

                                                                              
export function detectBankKey(text: string): BankKey | null {
  const n = normalizeCatalogSearch(text);
  const compact = n.replace(/\s+/g, "");
  if (
    compact.includes("рокет")
    || compact.includes("rocket")
    || n.includes("rocket bank")
  ) {
    return "rocketbank";
  }
  if (
    compact.includes("совком")
    || compact.includes("sovcom")
    || compact.includes("халва")
    || compact.includes("halva")
  ) {
    return "sovcombank";
  }
  return null;
}

export function detectBankFromApp(app: Pick<CatalogApp, "id" | "title" | "aliases">): BankKey | null {
  const parts = [app.id, app.title, ...(app.aliases ?? [])];
  for (const p of parts) {
    const hit = detectBankKey(p);
    if (hit) return hit;
  }
  return null;
}

export function bankDisplayName(bank: BankKey): string {
  return bank === "rocketbank" ? "Рокетбанк" : "Совкомбанк";
}

                                                   
export function bankIconUrl(bank: BankKey): string {
  return bank === "rocketbank"
    ? "https://cdn.appbank.pw/normal/rocket/main.png"
    : "https://cdn.appbank.pw/fake/sovcom/main.png";
}

                                                                
export function bankMinIos(bank: BankKey): string {
  return bank === "rocketbank" ? "16.0" : "17.0";
}

                                                                              
export function iosMeetsMin(deviceIos: string | null | undefined, minIos: string): boolean | null {
  const device = (deviceIos || "").trim();
  const min = minIos.trim();
  if (!device || !min) return null;
  const parts = (value: string) =>
    value.split(/\D+/).filter(Boolean).map(v => Number.parseInt(v, 10) || 0);
  const a = parts(device);
  const b = parts(min);
  if (!a.length || !b.length) return null;
  const len = Math.max(a.length, b.length);
  for (let i = 0; i < len; i += 1) {
    if ((a[i] ?? 0) < (b[i] ?? 0)) return false;
    if ((a[i] ?? 0) > (b[i] ?? 0)) return true;
  }
  return true;
}
