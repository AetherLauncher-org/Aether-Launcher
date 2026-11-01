import { createContext, useContext } from "react";
import { bundledCatalogs, defaultLocale } from "./catalogs";
import type { Locale, MessageKey } from "./catalogs";
import type { LocalePreference } from "./locale";
import { translate } from "./messages";
import type { TranslateParams } from "./messages";

export type TFunction = (key: MessageKey, params?: TranslateParams) => string;

export type I18nValue = {
  /** 实际在用的语言（「跟随系统」已经换算过） */
  locale: Locale;
  /** 用户的选择：一种语言，或 system */
  preference: LocalePreference;
  setPreference: (preference: LocalePreference) => void;
  /** 同 setPreference，保留给只会选具体语言的调用方 */
  setLocale: (locale: Locale) => void;
  t: TFunction;
};

export const defaultValue: I18nValue = {
  locale: defaultLocale,
  preference: defaultLocale,
  setPreference: () => undefined,
  setLocale: () => undefined,
  t: (key, params) =>
    translate(bundledCatalogs, defaultLocale, defaultLocale, key, params),
};

export const I18nContext = createContext<I18nValue>(defaultValue);

export function useT(): I18nValue {
  return useContext(I18nContext);
}
