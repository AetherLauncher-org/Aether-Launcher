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
