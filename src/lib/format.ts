export function formatMinIos(minIos?: string): string | null {
  const value = minIos?.trim();
  if (!value) return null;
  return `iOS ${value}+`;
}

export function formatVersionWithIos(version?: string, minIos?: string): string {
  const ver = version?.trim();
  const ios = formatMinIos(minIos);
  if (ver && ios) return `v${ver} · ${ios}`;
  if (ver) return `v${ver}`;
  if (ios) return ios;
  return "Версия уточняется";
}

export function formatCatalogCardVersion(
  app: Pick<{ version?: string; min_ios?: string }, "version" | "min_ios">,
): string {
  const formatted = formatVersionWithIos(app.version, app.min_ios);
  if (formatted !== "Версия уточняется") return formatted;
  return "Версия уточняется";
}

/** Русское склонение по числу: plural(1, "задача", "задачи", "задач"). */
export function plural(count: number, one: string, few: string, many: string): string {
  const n = Math.abs(count) % 100;
  const n1 = n % 10;
  if (n > 10 && n < 20) return many;
  if (n1 > 1 && n1 < 5) return few;
  if (n1 === 1) return one;
  return many;
}

/** Число вместе со словом: countLabel(3, "задача", "задачи", "задач") → «3 задачи». */
export function countLabel(count: number, one: string, few: string, many: string): string {
  return `${count} ${plural(count, one, few, many)}`;
}
