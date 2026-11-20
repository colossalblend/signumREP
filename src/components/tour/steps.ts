import type { NavigateFunction } from "react-router-dom";

type Ctx = {
  navigate: NavigateFunction;
  signal: (name: string) => void;
};

type AnimCtx = Ctx & {
  wait: (ms: number) => Promise<void>;
  ghostTo: (anchor: string) => Promise<void>;
};

export type TourStep = {
  /** `data-tour` value of the element to light up. */
  anchor: string;
  eyebrow: string;
  title: string;
  text: string;
  pad?: number;
  radius?: number;
  prefer?: "right" | "left" | "below" | "above";
  /** Puts the app into this step's end state. Always runs, both directions. */
  state: (ctx: Ctx) => void;
  /** Forward-only transition. Interrupted if the user presses «Далее». */
  anim?: (ctx: AnimCtx) => Promise<void>;
};

const catalog = ({ navigate, signal }: Ctx, sheet: boolean) => {
  navigate("/");
  signal(sheet ? "offer-open" : "offer-close");
};

export const STEPS: TourStep[] = [
  {
    anchor: "catalog-demo-card",
    eyebrow: "Каталог",
    title: "Начните с Каталога",
    text: "Здесь собраны популярные приложения. Для обучения мы добавили «Тест приложение» — оно исчезнет, когда вы закроете инструкцию.",
    pad: 10,
    prefer: "right",
    state: ctx => catalog(ctx, false),
  },
  {
    anchor: "install-sheet",
    eyebrow: "Шаг 1",
    title: "Откройте карточку",
    text: "Нажмите на приложение — Signum спросит Apple, какие версии доступны на вашем Apple ID, и откроет окно установки.",
    pad: 10,
    prefer: "left",
    state: ctx => catalog(ctx, true),
    async anim(ctx) {
      catalog(ctx, false);
      await ctx.wait(120);
      await ctx.ghostTo("catalog-demo-card");
      ctx.signal("offer-loading");
      await ctx.wait(750);
      ctx.signal("offer-open");
      await ctx.wait(260);
    },
  },
  {
    anchor: "sheet-versions",
    eyebrow: "Выбор версии",
    title: "Любая версия из покупок",
    text: "Signum показывает все сборки, привязанные к вашему Apple ID. Пометка «В покупках» значит, что версию можно поставить.",
    prefer: "left",
    state: ctx => catalog(ctx, true),
  },
  {
    anchor: "sheet-install",
    eyebrow: "Установка",
    title: "«Установить на iPhone»",
    text: "Скачает выбранную версию, подпишет её и поставит на подключённый телефон — всё одной кнопкой.",
    pad: 7,
    radius: 14,
    prefer: "left",
    state: ctx => catalog(ctx, true),
  },
  {
    anchor: "sheet-export",
    eyebrow: "Файл",
    title: "«Скачать файл»",
    text: "Сохранит готовый IPA на компьютер, без установки. Пригодится, чтобы поставить позже или на другой телефон.",
    pad: 7,
    radius: 14,
    prefer: "left",
    state: ctx => catalog(ctx, true),
  },
  {
    anchor: "sheet-other",
    eyebrow: "Ещё версии",
    title: "«Выбрать другую версию»",
    text: "Полный список сборок из покупок Apple ID — со значками совместимости с вашей iOS.",
    pad: 7,
    radius: 14,
    prefer: "left",
    state: ctx => catalog(ctx, true),
  },
  {
    anchor: "install-job",
    eyebrow: "Установки",
    title: "Следите за прогрессом",
    text: "Загрузка, подпись и установка идут в очереди. Здесь же видно ошибку, если что-то пойдёт не так.",
    prefer: "below",
    state: ({ navigate, signal }) => {
      signal("offer-close");
      navigate("/installs");
      signal("job-done");
    },
    async anim(ctx) {
      await ctx.ghostTo("sheet-install");
      ctx.signal("offer-close");
      ctx.navigate("/installs");
      ctx.signal("job-run");
      await ctx.wait(2400);
    },
  },
  {
    anchor: "nav-find",
    eyebrow: "Нет в каталоге?",
    title: "Переходите в «Поиск»",
    text: "Если нужного приложения нет в Каталоге — его можно найти в App Store по названию, ссылке или номеру.",
    pad: 6,
    radius: 14,
    prefer: "right",
    state: ({ navigate, signal }) => {
      navigate("/installs");
      signal("job-done");
    },
  },
  {
    anchor: "find-input",
    eyebrow: "Поиск",
    title: "Введите название",
    text: "Поиск идёт на лету — кнопку нажимать не нужно. Подойдёт и ссылка из App Store, и числовой ID.",
    prefer: "below",
    state: ({ navigate, signal }) => {
      navigate("/find");
      signal("find-results");
    },
    async anim(ctx) {
      await ctx.ghostTo("nav-find");
      ctx.navigate("/find");
      ctx.signal("find-typing");
      await ctx.wait(1500);
      ctx.signal("find-results");
      await ctx.wait(260);
    },
  },
  {
    anchor: "find-hit",
    eyebrow: "Результат",
    title: "Откройте найденное",
    text: "Карточка из поиска открывает то же окно установки — с выбором версии и теми же кнопками.",
    prefer: "below",
    state: ({ navigate, signal }) => {
      navigate("/find");
      signal("find-results");
    },
  },
  {
    anchor: "account-credentials",
    eyebrow: "Аккаунт",
    title: "Войдите в Apple ID",
    text: "Email и пароль от Apple ID. Signum не хранит пароль открытым — он лежит в хранилище системы.",
    prefer: "right",
    state: ({ navigate, signal }) => {
      navigate("/account");
      signal("account-credentials");
    },
    async anim(ctx) {
      await ctx.ghostTo("find-hit");
      ctx.navigate("/account");
      ctx.signal("account-typing");
      await ctx.wait(1600);
      ctx.signal("account-credentials");
    },
  },
  {
    anchor: "account-2fa",
    eyebrow: "Подтверждение",
    title: "Код из шести цифр",
    text: "Apple пришлёт код на доверенный iPhone или по SMS. Введите его здесь — и аккаунт подключён.",
    prefer: "right",
    state: ({ navigate, signal }) => {
      navigate("/account");
      signal("account-2fa");
    },
  },
  {
    anchor: "account-country",
    eyebrow: "Страна",
    title: "Магазин Apple ID",
    text: "От страны зависит, какие приложения доступны. Signum умеет временно сменить её и вернуть обратно.",
    prefer: "right",
    state: ({ navigate, signal }) => {
      navigate("/account");
      signal("account-2fa");
    },
  },
  {
    anchor: "account-history",
    eyebrow: "История покупок",
    title: "Не помните название?",
    text: "Здесь все приложения, которые вы когда-либо скачивали на этот Apple ID. Отметьте нужные и ставьте сразу пачкой.",
    prefer: "right",
    state: ({ navigate, signal }) => {
      navigate("/account");
      signal("account-2fa");
    },
  },
  {
    anchor: "ipa-file",
    eyebrow: "IPA-файл",
    title: "Signum читает файл сам",
    text: "Выберите IPA — Signum сам определит тип. Такому файлу нужна подпись вашим Apple ID; зашифрованный из App Store ставится сразу, подписывать не надо.",
    prefer: "right",
    state: ({ navigate }) => navigate("/ipa-signing"),
    async anim(ctx) {
      await ctx.ghostTo("nav-ipa");
      ctx.navigate("/ipa-signing");
      await ctx.wait(260);
    },
  },
  {
    anchor: "ipa-cert",
    eyebrow: "Подпись",
    title: "Бесплатно или своим сертификатом",
    text: "Через Apple ID — быстро, но приложение живёт 7 дней. Свой сертификат разработчика даёт срок до года.",
    prefer: "right",
    state: ({ navigate }) => navigate("/ipa-signing"),
  },
];
