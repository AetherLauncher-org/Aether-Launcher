import zhCN from "../../locales/zh-CN.json";
import enUS from "../../locales/en-US.json";
import type { MessageCatalog } from "./messages";

export const bundledCatalogs = {
  "zh-CN": zhCN as MessageCatalog,
  "en-US": enUS as MessageCatalog,
} as const satisfies Readonly<Record<string, MessageCatalog>>;

export type Locale = keyof typeof bundledCatalogs;

export const defaultLocale: Locale = "zh-CN";

export const locales: readonly Locale[] = ["zh-CN", "en-US"];

export const localeDisplayNames: Readonly<Record<Locale, string>> = {
  "zh-CN": "简体中文",
  "en-US": "English",
};

type CatalogKey = keyof typeof zhCN & string;
type PluralStem<K extends string> = K extends `${infer Stem}.other` ? Stem : never;
export type MessageKey = CatalogKey | PluralStem<CatalogKey>;
