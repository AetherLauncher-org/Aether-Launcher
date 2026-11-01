import type { Locale } from "./catalogs";
import { defaultLocale, locales } from "./catalogs";

export const localeStorageKey = "aether.locale";

export function isLocale(value: unknown): value is Locale {
  return (
    typeof value === "string" &&
    (locales as readonly string[]).includes(value)
  );
}

export function loadLocale(): Locale {
  try {
    const stored = window.localStorage.getItem(localeStorageKey);
    return isLocale(stored) ? stored : defaultLocale;
  } catch {
    return defaultLocale;
  }
}

export function saveLocale(locale: Locale): void {
  try {
    window.localStorage.setItem(localeStorageKey, locale);
  } catch {
    // The selection remains active for this session.
  }
}
