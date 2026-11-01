import { useCallback, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { bundledCatalogs, defaultLocale } from "./catalogs";
import { I18nContext } from "./context";
import type { I18nValue, TFunction } from "./context";
import { loadPreference, resolveLocale, savePreference } from "./locale";
import type { LocalePreference } from "./locale";
import { translate } from "./messages";

export function I18nProvider({ children }: { children: ReactNode }) {
  const [preference, setPreferenceState] = useState<LocalePreference>(loadPreference);
  // 系统语言变了（浏览器里改了首选语言）时，「跟随系统」跟着变
  const [systemTick, setSystemTick] = useState(0);
  const locale = useMemo(
    () => resolveLocale(preference),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- systemTick 只用来重新读系统语言
    [preference, systemTick],
  );

  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);

  useEffect(() => {
    const onChange = () => setSystemTick((tick) => tick + 1);
    window.addEventListener("languagechange", onChange);
    return () => window.removeEventListener("languagechange", onChange);
  }, []);

  const setPreference = useCallback((next: LocalePreference) => {
    setPreferenceState(next);
    savePreference(next);
  }, []);

  const t = useCallback<TFunction>(
    (key, params) =>
      translate(bundledCatalogs, defaultLocale, locale, key, params),
    [locale],
  );

  const value = useMemo<I18nValue>(
    () => ({ locale, preference, setPreference, setLocale: setPreference, t }),
    [locale, preference, setPreference, t],
  );

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}
