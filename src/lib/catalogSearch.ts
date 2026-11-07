import type { CatalogApp } from "../types";

const ALIAS_GROUPS = [
  ["альфа банк", "альфабанк", "alfa bank", "alfabank", "alpha bank"],
  ["промсвязьбанк", "пром связь банк", "псб", "psb", "promsvyazbank", "promsviazbank"],
  ["газпромбанк", "газпром банк", "гпб", "gpb", "gazprombank"],
  ["т банк", "тбанк", "тинькофф", "t bank", "tbank", "tinkoff"],
  ["сбербанк", "сбер банк", "сбер", "sberbank", "sber"],
  ["сбер инвестиции", "сберинвестиции", "сбер инвест", "sber invest", "sber investments"],
  ["втб", "vtb", "внешторгбанк"],
  ["россельхозбанк", "россельхоз банк", "рсхб", "rshb", "rosselkhozbank"],
  ["райффайзенбанк", "райффайзен банк", "райф", "raiffeisenbank", "raiffeisen"],
  ["московский кредитный банк", "мкб", "mkb", "credit bank of moscow"],
  ["совкомбанк", "совком банк", "халва", "sovcombank", "sovcom"],
  ["рокетбанк", "рокет банк", "rocketbank", "rocket bank", "рокет"],
  ["почта банк", "почтабанк", "pochtabank", "post bank"],
  ["озон банк", "озонбанк", "ozon bank", "ozonbank"],
] as const;

const CYRILLIC_TO_LATIN: Record<string, string> = {
  а: "a", б: "b", в: "v", г: "g", д: "d", е: "e", ж: "zh", з: "z", и: "i",
  й: "y", к: "k", л: "l", м: "m", н: "n", о: "o", п: "p", р: "r", с: "s",
  т: "t", у: "u", ф: "f", х: "h", ц: "ts", ч: "ch", ш: "sh", щ: "sch",
  ъ: "", ы: "y", ь: "", э: "e", ю: "yu", я: "ya",
};

export function normalizeCatalogSearch(value: string): string {
  return value
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLocaleLowerCase("ru-RU")
    .replace(/ё/g, "е")
    .replace(/[^a-zа-я0-9]+/g, " ")
    .trim()
    .replace(/\s+/g, " ");
}

function transliterate(value: string): string {
  return [...value].map(char => CYRILLIC_TO_LATIN[char] ?? char).join("");
}

function compact(value: string): string {
  return value.replace(/\s+/g, "");
}

function addVariant(target: Set<string>, value: string): void {
  const normalized = normalizeCatalogSearch(value);
  if (!normalized) return;
  target.add(normalized);
  target.add(compact(normalized));
  const latin = normalizeCatalogSearch(transliterate(normalized));
  if (latin) {
    target.add(latin);
    target.add(compact(latin));
  }
}

function phraseMatches(value: string, alias: string): boolean {
  const normalizedValue = normalizeCatalogSearch(value);
  const normalizedAlias = normalizeCatalogSearch(alias);
  const valueCompact = compact(normalizedValue);
  const aliasCompact = compact(normalizedAlias);
  return normalizedValue === normalizedAlias
    || valueCompact === aliasCompact
    || (aliasCompact.length >= 4 && valueCompact.includes(aliasCompact));
}

function expandAliases(values: string[]): Set<string> {
  const result = new Set<string>();
  values.forEach(value => addVariant(result, value));

  for (const group of ALIAS_GROUPS) {
    if (values.some(value => group.some(alias => phraseMatches(value, alias)))) {
      group.forEach(alias => addVariant(result, alias));
    }
  }
  return result;
}

function editDistance(left: string, right: string): number {
  if (left === right) return 0;
  if (!left.length) return right.length;
  if (!right.length) return left.length;

  let previous = Array.from({ length: right.length + 1 }, (_, index) => index);
  for (let i = 1; i <= left.length; i += 1) {
    const current = [i];
    for (let j = 1; j <= right.length; j += 1) {
      current[j] = Math.min(
        current[j - 1] + 1,
        previous[j] + 1,
        previous[j - 1] + (left[i - 1] === right[j - 1] ? 0 : 1),
      );
    }
    previous = current;
  }
  return previous[right.length];
}

function fuzzyMatch(query: string, candidate: string): boolean {
  const left = compact(query);
  const right = compact(candidate);
  if (left.length < 4 || right.length < 4) return false;
  const allowance = Math.min(3, Math.max(1, Math.floor(Math.max(left.length, right.length) * 0.25)));
  return Math.abs(left.length - right.length) <= allowance && editDistance(left, right) <= allowance;
}

/** Самая дорогая ветка оценки — нечёткое сравнение; выше 450 она не поднимает. */
const FUZZY_CEILING = 450;

function relevance(query: Set<string>, terms: Set<string>): number {
  let best = 0;
  for (const search of query) {
    const searchTokens = search.split(" ").filter(Boolean);
    for (const term of terms) {
      // Точное совпадение — выше уже некуда, дальше считать нечего.
      if (term === search) return 1000;
      if (term.startsWith(search)) { best = Math.max(best, 850); continue; }
      if (term.includes(search)) { best = Math.max(best, 700); continue; }
      const termTokens = tokensOf(term);
      if (
        searchTokens.length > 1
        && searchTokens.every(token => termTokens.some(part => part.startsWith(token)))
      ) { best = Math.max(best, 650); continue; }
      // Редакционное расстояние стоит дороже всего остального вместе взятого.
      // Если что-то уже набрало больше её потолка, считать её незачем.
      if (best >= FUZZY_CEILING) continue;
      if (fuzzyMatch(search, term)) { best = Math.max(best, FUZZY_CEILING); continue; }
      if (searchTokens.every(token => termTokens.some(part => fuzzyMatch(token, part)))) {
        best = Math.max(best, 300);
      }
    }
  }
  return best;
}

/** Разбиение терма на слова — считалось заново на каждую пару запрос-терм. */
const tokenCache = new Map<string, string[]>();

function tokensOf(term: string): string[] {
  let tokens = tokenCache.get(term);
  if (!tokens) {
    tokens = term.split(" ").filter(Boolean);
    tokenCache.set(term, tokens);
  }
  return tokens;
}

/**
 * Развёрнутые синонимы приложения.
 *
 * Считались для каждого приложения на каждое нажатие клавиши, прогоняя все
 * группы синонимов, — при том что у приложения они не меняются, пока каталог
 * не перезагрузится. Ключ — сам объект приложения, поэтому новая загрузка
 * каталога сбрасывает кеш сама.
 */
const variantCache = new WeakMap<CatalogApp, Set<string>>();

function appVariants(app: CatalogApp): Set<string> {
  let variants = variantCache.get(app);
  if (!variants) {
    variants = expandAliases([app.title, app.id, ...(app.aliases ?? [])]);
    variantCache.set(app, variants);
  }
  return variants;
}

export function searchCatalogApps(apps: CatalogApp[], rawQuery: string): CatalogApp[] {
  const normalizedQuery = normalizeCatalogSearch(rawQuery);
  if (!normalizedQuery) return apps;

  const query = expandAliases([normalizedQuery]);
  return apps
    .map((app, index) => ({
      app,
      index,
      score: relevance(query, appVariants(app)),
    }))
    .filter(item => item.score > 0)
    .sort((left, right) => right.score - left.score || left.index - right.index)
    .map(item => item.app);
}
