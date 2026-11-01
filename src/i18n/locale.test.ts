import { afterEach, describe, expect, it } from "vitest";
import {
  isLocale,
  isPreference,
  loadPreference,
  localeStorageKey,
  resolveLocale,
  savePreference,
  systemLocale,
} from "./locale";

describe("locale preference", () => {
  afterEach(() => {
    window.localStorage.clear();
  });

  it("follows the system when nothing is stored", () => {
    expect(loadPreference()).toBe("system");
  });

  it("remembers a saved choice, including following the system again", () => {
    savePreference("en-US");
    expect(window.localStorage.getItem(localeStorageKey)).toBe("en-US");
    expect(loadPreference()).toBe("en-US");
    savePreference("system");
    expect(loadPreference()).toBe("system");
  });

  it("ignores an unknown stored value", () => {
    window.localStorage.setItem(localeStorageKey, "fr-FR");
    expect(loadPreference()).toBe("system");
  });

  it("type guards accept only registered choices", () => {
    expect(isLocale("zh-CN")).toBe(true);
    expect(isLocale("system")).toBe(false);
    expect(isPreference("system")).toBe(true);
    expect(isPreference("en-US")).toBe(true);
    expect(isPreference("fr-FR")).toBe(false);
    expect(isPreference(null)).toBe(false);
    expect(isPreference(42)).toBe(false);
  });
});

describe("system language", () => {
  it("maps Chinese to zh-CN and English to en-US, whatever the region", () => {
    expect(systemLocale(["zh-CN"])).toBe("zh-CN");
    expect(systemLocale(["zh-TW"])).toBe("zh-CN");
    expect(systemLocale(["zh"])).toBe("zh-CN");
    expect(systemLocale(["en-GB"])).toBe("en-US");
    expect(systemLocale(["EN"])).toBe("en-US");
  });

  it("takes the first language it knows, in the system's order", () => {
    expect(systemLocale(["fr-FR", "zh-CN", "en-US"])).toBe("zh-CN");
    expect(systemLocale(["fr-FR", "en-US", "zh-CN"])).toBe("en-US");
  });

  it("falls back to English for an unsupported system and to the default for no information", () => {
    expect(systemLocale(["fr-FR", "de"])).toBe("en-US");
    expect(systemLocale([])).toBe("zh-CN");
  });

  it("resolves a fixed choice as is and system by language", () => {
    expect(resolveLocale("zh-CN", ["en-US"])).toBe("zh-CN");
    expect(resolveLocale("system", ["en-US"])).toBe("en-US");
  });
});
