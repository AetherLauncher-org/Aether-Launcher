import { useCallback, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { bundledCatalogs, defaultLocale } from "./catalogs";
import type { Locale } from "./catalogs";
import { I18nContext } from "./context";
import type { I18nValue, TFunction } from "./context";
import { loadLocale, saveLocale } from "./locale";
import { translate } from "./messages";

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>(loadLocale);

  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);

  const setLocale = useCallback((next: Locale) => {
    setLocaleState(next);
    saveLocale(next);
  }, []);

  const t = useCallback<TFunction>(
    (key, params) =>
      translate(bundledCatalogs, defaultLocale, locale, key, params),
    [locale],
  );

  const value = useMemo<I18nValue>(
    () => ({ locale, setLocale, t }),
    [locale, setLocale, t],
  );

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}
