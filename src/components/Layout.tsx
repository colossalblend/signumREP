import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  BadgeCheck, LayoutGrid, Maximize2, Minus, MonitorSmartphone, Search, Settings,
  Smartphone, UserRound, X,
} from "lucide-react";
import { useEffect, useState } from "react";
import { NavLink, Outlet } from "react-router-dom";
import signumIcon from "../../assets/signum-icon.png";
import { applyAppearance } from "../lib/appearance";
import { command, setDetailedErrors } from "../lib/tauri";
import type { AppSettings, AppVersionInfo } from "../types";
import ClientOutdatedGate from "./ClientOutdatedGate";

import DragRegion from "./DragRegion";
import IcloudSetupGate from "./IcloudSetupGate";
import { hasSeenOnboarding } from "../lib/tourStore";
import TourBoundary from "./tour/TourBoundary";
import TourOverlay from "./tour/TourOverlay";
import Signing2faGate from "./Signing2faGate";
import UpdateBanners from "./UpdateBanners";

const nav = [
  ["/", "Каталог", LayoutGrid, "nav-catalog"],
  ["/find", "Поиск", Search, "nav-find"],
  ["/ipa-signing", "IPA-файл", BadgeCheck, "nav-ipa"],
  ["/devices", "Устройства", Smartphone, "nav-devices"],
  ["/account", "Аккаунт", UserRound, "nav-account"],
  ["/installs", "Установки", MonitorSmartphone, "nav-installs"],
] as const;

function WindowButton({ children, onClick, danger = false, label }: {
  children: React.ReactNode; onClick: () => void; danger?: boolean; label: string;
}) {
  return (
    <button
      aria-label={label}
      onClick={onClick}
      data-no-drag
      className={`grid h-11 w-12 place-items-center transition ${danger ? "hover:bg-red-500/80" : "hover:bg-black/5 dark:hover:bg-white/[0.07]"}`}
    >
      {children}
    </button>
  );
}

export default function Layout() {
  const window = getCurrentWindow();
  const [appVersion, setAppVersion] = useState("1.1.1");
  const [showOnboarding, setShowOnboarding] = useState(() => !hasSeenOnboarding());

  useEffect(() => {
    command<AppSettings>("get_settings")
      .then(settings => {
        setDetailedErrors(settings.detailed_errors);
        applyAppearance(settings);
      })
      .catch(() => undefined);
    command<AppVersionInfo>("get_app_version_info")
      .then(info => setAppVersion(info.version))
      .catch(() => undefined);
  }, []);

  const isMac = typeof navigator !== 'undefined' && /Mac/.test(navigator.platform);

  return (
    <div className={`app-shell flex h-full ${isMac ? 'is-mac' : ''}`}>
      {showOnboarding && (
        <TourBoundary onClose={() => setShowOnboarding(false)}>
          <TourOverlay onClose={() => setShowOnboarding(false)} />
        </TourBoundary>
      )}
      <aside className="app-sidebar relative z-20 flex w-[236px] shrink-0 flex-col px-4 pb-4">
        <DragRegion className="flex h-[76px] w-full cursor-grab items-center gap-3 px-2 pt-0 active:cursor-grabbing is-mac:pt-7">
          <img src={signumIcon} alt="Signum" draggable={false} className="pointer-events-none h-10 w-10 rounded-xl object-cover shadow-glow" />
          <div className="pointer-events-none">
            <div className="font-bold tracking-tight text-theme">Signum</div>
            <div className="text-[10px] font-medium text-muted">{appVersion}</div>
          </div>
        </DragRegion>

        <nav className="mt-5 space-y-1.5" data-no-drag>
          {nav.map(([to, label, Icon, anchor]) => (
            <NavLink
              key={to}
              to={to}
              end={to === "/"}
              data-tour={anchor}
              className={({ isActive }) =>
                `flex items-center gap-3 rounded-xl px-3.5 py-3 text-sm font-medium transition ${isActive ? "nav-link-active" : "nav-link hover:bg-black/5 dark:hover:bg-white/[0.04]"}`}
            >
              <Icon size={18} strokeWidth={2} />{label}
            </NavLink>
          ))}
        </nav>

        <div className="mt-auto" data-no-drag>
          <NavLink
            to="/settings"
            className={({ isActive }) =>
              `flex items-center gap-3 rounded-xl px-3.5 py-3 text-sm font-medium transition ${isActive ? "nav-link-active" : "nav-link hover:bg-black/5 dark:hover:bg-white/[0.04]"}`}
          >
            <Settings size={18} />Настройки
          </NavLink>
        </div>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="app-header relative z-30 flex h-11 shrink-0 items-stretch">
          {                                                                                            }
          <DragRegion className="h-11 min-h-[44px] min-w-0 flex-1 cursor-grab active:cursor-grabbing" />
          <div className="flex shrink-0 items-center" data-no-drag>
            <WindowButton label="Свернуть" onClick={() => void window.minimize()}><Minus size={16} /></WindowButton>
            <WindowButton label="Развернуть" onClick={() => void window.toggleMaximize()}><Maximize2 size={15} strokeWidth={1.75} /></WindowButton>
            <WindowButton label="Закрыть" danger onClick={() => void window.close()}><X size={17} /></WindowButton>
          </div>
        </header>
        <UpdateBanners />
        <IcloudSetupGate />
        <main className="min-h-0 flex-1 overflow-y-auto">
          <Outlet />
        </main>
      </div>
      <ClientOutdatedGate />
      <Signing2faGate />
    </div>
  );
}
