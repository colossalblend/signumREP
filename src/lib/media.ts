                                                                                  
export function cacheBustUrl(url: string | undefined | null, stamp: number | string): string {
  if (!url?.trim()) return "";
  if (String(url).includes("signum_cb=")) return url;
  const sep = url.includes("?") ? "&" : "?";
  return `${url}${sep}signum_cb=${stamp}`;
}
