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
