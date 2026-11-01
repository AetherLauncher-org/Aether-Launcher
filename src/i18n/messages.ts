export type MessageCatalog = Readonly<Record<string, string>>;
export type TranslateParams = Readonly<Record<string, string | number>>;

export function formatMessage(template: string, params?: TranslateParams): string {
  if (params === undefined) return template;
  return template.replace(/\{([a-zA-Z_][a-zA-Z0-9_]*)\}/g, (whole, name: string) =>
    Object.prototype.hasOwnProperty.call(params, name) ? String(params[name]) : whole,
  );
}

function lookup(
  catalog: MessageCatalog | undefined,
  key: string,
  count: number | undefined,
  locale: string,
): string | undefined {
  if (catalog === undefined) return undefined;
  if (count === undefined) return catalog[key];
  const category = new Intl.PluralRules(locale).select(count);
  return catalog[`${key}.${category}`] ?? catalog[`${key}.other`] ?? catalog[key];
}

export function translate(
  catalogs: Readonly<Record<string, MessageCatalog>>,
  fallbackLocale: string,
  locale: string,
  key: string,
  params?: TranslateParams,
): string {
  const count = typeof params?.count === "number" ? params.count : undefined;
  const template = lookup(catalogs[locale], key, count, locale) ??
    (locale !== fallbackLocale ? lookup(catalogs[fallbackLocale], key, count, fallbackLocale) : undefined) ?? key;
  return formatMessage(template, params);
}

export function createT(
  catalogs: Readonly<Record<string, MessageCatalog>>,
  fallbackLocale: string,
  locale: string,
): (key: string, params?: TranslateParams) => string {
  return (key, params) => translate(catalogs, fallbackLocale, locale, key, params);
}
