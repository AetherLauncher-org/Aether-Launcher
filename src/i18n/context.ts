import { createContext, useContext } from "react";
import { bundledCatalogs, defaultLocale } from "./catalogs";
import type { Locale, MessageKey } from "./catalogs";
import { translate } from "./messages";
import type { TranslateParams } from "./messages";

export type TFunction = (key: MessageKey, params?: TranslateParams) => string;

export type I18nValue = {
  locale: Locale;
  setLocale: (locale: Locale) => void;
  t: TFunction;
};

/**
 * 默认值用源语言 zh-CN：不包 `I18nProvider` 也能用 `useT()`（现有组件测试就
 * 直接渲染组件、没包 Provider），此时文案保持中文，和现状一致。
 */
export const defaultValue: I18nValue = {
  locale: defaultLocale,
  setLocale: () => undefined,
  t: (key, params) =>
    translate(bundledCatalogs, defaultLocale, defaultLocale, key, params),
};

export const I18nContext = createContext<I18nValue>(defaultValue);

export function useT(): I18nValue {
  return useContext(I18nContext);
}
