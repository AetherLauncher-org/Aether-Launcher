/**
 * 纯翻译逻辑：不依赖 React、不依赖打包进来的目录，方便单测。
 *
 * 占位符语法是 ICU 的子集：只支持 `{name}`（`name` 为 `[a-zA-Z_][a-zA-Z0-9_]*`），
 * 不支持 `{count, plural, ...}` 内联选择器和转义。复数用「词干 + 类别键」的方式
 * 外部化，见 `translate`。
 */

/** 一整个语言的目录：扁平、点分键，值为字符串。 */
export type MessageCatalog = Readonly<Record<string, string>>;

/** 传给 `t()` 的可插值参数。 */
export type TranslateParams = Readonly<Record<string, string | number>>;

/** `Intl.PluralRules` 的类别，按常用程度排序。 */
const PLURAL_CATEGORIES = [
  "zero",
  "one",
  "two",
  "few",
  "many",
  "other",
] as const;

/** 复数词干：`download.fileCount` 这类不含类别后缀、配 `{ count }` 调用的键。 */
export type PluralStem = string;

/** 复数类别后缀集合（去重）。 */
const PLURAL_SUFFIXES = new Set<string>(PLURAL_CATEGORIES);

/**
 * 把一个模板里的 `{name}` 占位符替换成参数值。缺失的参数和未知占位符都原样保留，
 * 让缺漏在界面上直接可见而不是被静默吞掉。
 */
export function formatMessage(
  template: string,
  params?: TranslateParams,
): string {
  if (params === undefined) {
    return template;
  }
  return template.replace(
    /\{([a-zA-Z_][a-zA-Z0-9_]*)\}/g,
    (whole, name: string) =>
      Object.prototype.hasOwnProperty.call(params, name)
        ? String(params[name])
        : whole,
  );
}

/** 在单个目录里按复数规则取模板。`count` 为数字时走复数词干，否则按普通键取。 */
function lookup(
  catalog: MessageCatalog | undefined,
  key: string,
  count: number | undefined,
  locale: string,
): string | undefined {
  if (catalog === undefined) {
    return undefined;
  }
  if (count === undefined) {
    return catalog[key];
  }
  const category = new Intl.PluralRules(locale).select(count);
  return catalog[`${key}.${category}`] ?? catalog[`${key}.other`];
}

/**
 * 核心翻译：先当前语言，再回退语言（默认 zh-CN），都没有就返回键名本身。
 *
 * `count` 是保留参数名：当 `params.count` 是数字时，按 `Intl.PluralRules` 取
 * `key.<类别>`，缺类别回退 `key.other`。
 */
export function translate(
  catalogs: Readonly<Record<string, MessageCatalog>>,
  fallbackLocale: string,
  locale: string,
  key: string,
  params?: TranslateParams,
): string {
  const count =
    typeof params?.count === "number" ? params.count : undefined;

  const template =
    lookup(catalogs[locale], key, count, locale) ??
    (locale !== fallbackLocale
      ? lookup(catalogs[fallbackLocale], key, count, fallbackLocale)
      : undefined) ??
    key;

  return formatMessage(template, params);
}

/**
 * 把一个目录绑定成一个无类型约束的翻译函数（单测、复用场景用）。
 * 应用里走 `useT()` 拿到带键类型约束的 `t`，见 `I18nProvider`。
 */
export function createT(
  catalogs: Readonly<Record<string, MessageCatalog>>,
  fallbackLocale: string,
  locale: string,
): (key: string, params?: TranslateParams) => string {
  return (key, params) => translate(catalogs, fallbackLocale, locale, key, params);
}

/** 判断一个键是否是复数词干（本身不是键，但 `key.<类别>` 存在）。 */
export function isPluralStem(catalog: MessageCatalog, key: string): boolean {
  for (const category of PLURAL_CATEGORIES) {
    if (catalog[`${key}.${category}`] !== undefined) {
      return true;
    }
  }
  return false;
}

/** 复数类别后缀，供 checker 复用。 */
export function pluralSuffixes(): readonly string[] {
  return PLURAL_CATEGORIES;
}

/** 判断一个键名是否以复数类别后缀结尾（checker 用它报结构警告）。 */
export function endsWithPluralSuffix(key: string): boolean {
  const last = key.slice(key.lastIndexOf(".") + 1);
  return PLURAL_SUFFIXES.has(last);
}
