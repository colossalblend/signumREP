import { ArrowLeft, ArrowUpRight, Globe, Send } from "lucide-react";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import signumIcon from "../../assets/signum-icon.png";
import { command, errorMessage } from "../lib/tauri";
import type { AppVersionInfo } from "../types";

const TELEGRAM_URL = "https://t.me/appbankRU";
const SITE_URL = "https://appbank.pw";
const IPA_DOWNLOADER_URL = "https://github.com/kda2495/IPA_Downloader";

type Library = { name: string; role: string; url: string };

/** Что лежит под капотом. Порядок — от того, что видит пользователь, к инфраструктуре. */
const LIBRARY_GROUPS: Array<{ title: string; items: Library[] }> = [
  {
    title: "Apple ID и App Store",
    items: [
      { name: "ipatool-cpp", role: "Вход в Apple ID, покупка, версии и скачивание IPA", url: "https://github.com/Sorvigolova/ipatool" },
      { name: "ipatool", role: "Резервный движок и история покупок без кода", url: "https://github.com/majd/ipatool" },
      { name: "sapsigner", role: "FairPlay и подпись запросов к App Store", url: "https://github.com/t0rr3sp3dr0/sapsigner" },
      { name: "Unicorn Engine", role: "Эмуляция для sapsigner и kbsync", url: "https://github.com/unicorn-engine/unicorn" },
    ],
  },
  {
    title: "iPhone и подпись",
    items: [
      { name: "libimobiledevice", role: "Сопряжение, сведения об устройстве, установка", url: "https://github.com/libimobiledevice/libimobiledevice" },
      { name: "isideload", role: "Подпись и установка из Rust", url: "https://github.com/nab138/isideload" },
      { name: "zsign", role: "Переподпись IPA", url: "https://github.com/zhlynn/zsign" },
    ],
  },
  {
    title: "Приложение",
    items: [
      { name: "Tauri", role: "Оболочка настольного приложения", url: "https://github.com/tauri-apps/tauri" },
      { name: "React", role: "Интерфейс", url: "https://github.com/facebook/react" },
      { name: "Tailwind CSS", role: "Стили", url: "https://github.com/tailwindlabs/tailwindcss" },
      { name: "Lucide", role: "Иконки", url: "https://github.com/lucide-icons/lucide" },
    ],
  },
];

function hostOf(url: string): string {
  return url.replace(/^https:\/\//, "").replace(/\/$/, "");
}

export default function About() {
  const navigate = useNavigate();
  const [version, setVersion] = useState<AppVersionInfo | null>(null);
  const [linkError, setLinkError] = useState("");

  useEffect(() => {
    command<AppVersionInfo>("get_app_version_info").then(setVersion).catch(() => undefined);
  }, []);

  async function openLink(url: string) {
    setLinkError("");
    try {
      await command("open_external_url", { url });
    } catch (e) {
      setLinkError(errorMessage(e));
    }
  }

  return (
    <div className="about-page mx-auto max-w-[860px] p-7 lg:p-9">
      <button
        type="button"
        onClick={() => navigate("/settings")}
        className="inline-flex items-center gap-1.5 rounded-lg px-2 py-1.5 text-xs font-semibold text-slate-500 transition hover:bg-white/[0.05] hover:text-white"
      >
        <ArrowLeft size={14} />Настройки
      </button>

      <header className="mt-5 flex items-center gap-5">
        <img src={signumIcon} alt="" draggable={false} className="h-[72px] w-[72px] rounded-[20px] object-cover shadow-glow" />
        <div className="min-w-0">
          <h1 className="page-title">Signum</h1>
          <p className="mt-1 text-sm text-slate-500">
            Установка приложений на iPhone с компьютера
            {version && <span className="ml-2 font-mono text-xs text-slate-600">{version.version} · build {version.build}{version.beta ? " · beta" : ""}</span>}
          </p>
        </div>
      </header>

      <section className="mt-8 grid gap-3 sm:grid-cols-2">
        <button type="button" className="about-link about-link-telegram" onClick={() => void openLink(TELEGRAM_URL)}>
          <span className="about-link-icon"><Send size={18} /></span>
          <span className="min-w-0 flex-1 text-left">
            <span className="block text-sm font-bold text-theme">Telegram-канал</span>
            <span className="block truncate text-xs text-slate-500">@appbankRU — новости и помощь</span>
          </span>
          <ArrowUpRight size={16} className="shrink-0 text-slate-500" />
        </button>
        <button type="button" className="about-link about-link-site" onClick={() => void openLink(SITE_URL)}>
          <span className="about-link-icon"><Globe size={18} /></span>
          <span className="min-w-0 flex-1 text-left">
            <span className="block text-sm font-bold text-theme">Сайт</span>
            <span className="block truncate text-xs text-slate-500">appbank.pw — загрузки для всех систем</span>
          </span>
          <ArrowUpRight size={16} className="shrink-0 text-slate-500" />
        </button>
      </section>

      {linkError && <p role="alert" className="mt-3 text-xs text-rose-300">{linkError}</p>}

      <section className="panel mt-8 p-6">
        <div className="text-[11px] font-bold uppercase tracking-[0.08em] text-slate-500">В основе</div>
        <div className="mt-3 flex flex-wrap items-start justify-between gap-4">
          <div className="min-w-0 max-w-xl">
            <div className="text-base font-bold text-theme">IPA Downloader</div>
            <p className="mt-1.5 text-xs leading-5 text-slate-400">
              Signum вырос из IPA Downloader: оттуда пришли сборки утилит для работы с iPhone
              на Windows и macOS и сама идея — ставить нужные версии приложений без джейлбрейка.
            </p>
          </div>
          <button type="button" className="btn-secondary shrink-0" onClick={() => void openLink(IPA_DOWNLOADER_URL)}>
             kda2495/IPA_Downloader · 4.5.0<ArrowUpRight size={14} />
          </button>
        </div>
      </section>

      <section className="mt-8">
        <h2 className="text-sm font-bold text-slate-200">Библиотеки и компоненты</h2>
        <p className="mt-1 text-xs text-slate-500">Открытые проекты, без которых Signum не работал бы. Спасибо их авторам.</p>
        <div className="mt-4 space-y-6">
          {LIBRARY_GROUPS.map(group => (
            <div key={group.title}>
              <div className="mb-2 text-[11px] font-bold uppercase tracking-[0.08em] text-slate-500">{group.title}</div>
              <ul className="panel divide-y divide-white/[0.06] overflow-hidden p-0">
                {group.items.map(lib => (
                  <li key={lib.name}>
                    <button
                      type="button"
                      onClick={() => void openLink(lib.url)}
                      className="flex w-full items-center gap-4 px-4 py-3 text-left transition hover:bg-white/[0.035]"
                    >
                      <span className="w-36 shrink-0 text-sm font-semibold text-theme">{lib.name}</span>
                      <span className="min-w-0 flex-1 truncate text-xs text-slate-500">{lib.role}</span>
                      <span className="hidden shrink-0 font-mono text-[11px] text-slate-600 md:inline">{hostOf(lib.url).replace(/^github\.com\//, "")}</span>
                      <ArrowUpRight size={14} className="shrink-0 text-slate-600" />
                    </button>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
      </section>

      <p className="mt-10 text-center text-[11px] leading-5 text-slate-600">
        Signum не связан с Apple Inc. App Store, iPhone и Apple ID — товарные знаки Apple Inc.
      </p>
    </div>
  );
}
