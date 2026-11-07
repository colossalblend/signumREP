import type { AppSettings } from "../types";

export type ResolvedTheme = "light" | "dark";

type AppearanceSettings = Pick<
  AppSettings,
  | "theme"
  | "lite_design"
  | "lite_static_loaders"
  | "lite_reduce_motion"
  | "lite_simple_scrolling"
  | "lite_disable_blur"
  | "lite_disable_shadows"
  | "lite_flat_backgrounds"
>;

let systemListener: ((event: MediaQueryListEvent) => void) | null = null;
let currentSettings: AppearanceSettings | null = null;

export function resolveTheme(theme: AppSettings["theme"]): ResolvedTheme {
  if (theme === "light") return "light";
  if (theme === "dark") return "dark";
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function updateThemeColor(resolved: ResolvedTheme) {
  const meta = document.querySelector('meta[name="theme-color"]');
  if (meta) {
    meta.setAttribute("content", resolved === "dark" ? "#0f1420" : "#f4f6fb");
  }
}

export function applyAppearance(settings: AppearanceSettings) {
  currentSettings = settings;
  const resolved = resolveTheme(settings.theme);
  const root = document.documentElement;

  root.classList.toggle("dark", resolved === "dark");
  root.classList.toggle("light", resolved === "light");
  root.classList.toggle("lite", settings.lite_design);
  root.classList.toggle("lite-static-loaders", settings.lite_design && settings.lite_static_loaders !== false);
  root.classList.toggle("lite-reduce-motion", settings.lite_design && settings.lite_reduce_motion !== false);
  root.classList.toggle("lite-simple-scrolling", settings.lite_design && settings.lite_simple_scrolling !== false);
  root.classList.toggle("lite-disable-blur", settings.lite_design && settings.lite_disable_blur !== false);
  root.classList.toggle("lite-disable-shadows", settings.lite_design && settings.lite_disable_shadows !== false);
  root.classList.toggle("lite-flat-backgrounds", settings.lite_design && settings.lite_flat_backgrounds !== false);
  root.style.colorScheme = resolved;
  updateThemeColor(resolved);

  if (systemListener) {
    window
      .matchMedia("(prefers-color-scheme: dark)")
      .removeEventListener("change", systemListener);
    systemListener = null;
  }

  if (settings.theme === "system") {
    systemListener = () => {
      if (currentSettings) {
        applyAppearance(currentSettings);
      }
    };
    window
      .matchMedia("(prefers-color-scheme: dark)")
      .addEventListener("change", systemListener);
  }
}
