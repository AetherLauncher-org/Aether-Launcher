import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "./I18nProvider";
import { useT } from "./context";
import { localeStorageKey } from "./locale";

function Probe() {
  const { locale, preference, setPreference, setLocale, t } = useT();
  return (
    <div>
      <span data-testid="preference">{preference}</span>
      <button type="button" onClick={() => setPreference("system")}>
        to-system
      </button>
      <span data-testid="nav">{t("nav.primary")}</span>
      <span data-testid="install">
        {t("titlebar.install.text", { version: "1.21.1", percent: 17 })}
      </span>
      <span data-testid="locale">{locale}</span>
      <button type="button" onClick={() => setLocale("en-US")}>
        to-en
      </button>
      <button type="button" onClick={() => setLocale("zh-CN")}>
        to-zh
      </button>
    </div>
  );
}

describe("I18nProvider", () => {
  afterEach(() => {
    window.localStorage.clear();
  });

  it("follows a Chinese system when nothing is chosen", () => {
    render(
      <I18nProvider>
        <Probe />
      </I18nProvider>,
    );
    expect(screen.getByTestId("preference")).toHaveTextContent("system");
    expect(screen.getByTestId("nav")).toHaveTextContent("主要导航");
    expect(screen.getByTestId("install")).toHaveTextContent("安装 1.21.1 · 17%");
  });

  it("follows an English system when nothing is chosen, and changes with it", () => {
    const languages = vi.spyOn(window.navigator, "languages", "get").mockReturnValue(["en-GB", "zh-CN"]);
    render(
      <I18nProvider>
        <Probe />
      </I18nProvider>,
    );
    expect(screen.getByTestId("locale")).toHaveTextContent("en-US");
    expect(screen.getByTestId("nav")).toHaveTextContent("Primary navigation");

    languages.mockReturnValue(["zh-CN"]);
    act(() => {
      window.dispatchEvent(new Event("languagechange"));
    });
    expect(screen.getByTestId("locale")).toHaveTextContent("zh-CN");
    languages.mockRestore();
  });

  it("goes back to following the system after a fixed choice", () => {
    render(
      <I18nProvider>
        <Probe />
      </I18nProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "to-en" }));
    expect(screen.getByTestId("preference")).toHaveTextContent("en-US");
    fireEvent.click(screen.getByRole("button", { name: "to-system" }));
    expect(screen.getByTestId("preference")).toHaveTextContent("system");
    expect(screen.getByTestId("locale")).toHaveTextContent("zh-CN");
    expect(window.localStorage.getItem(localeStorageKey)).toBe("system");
  });

  it("switches language, re-renders and persists", () => {
    render(
      <I18nProvider>
        <Probe />
      </I18nProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "to-en" }));

    expect(screen.getByTestId("locale")).toHaveTextContent("en-US");
    expect(screen.getByTestId("nav")).toHaveTextContent("Primary navigation");
    expect(screen.getByTestId("install")).toHaveTextContent("Install 1.21.1 · 17%");
    expect(window.localStorage.getItem(localeStorageKey)).toBe("en-US");
  });

  it("restores a saved locale on mount", () => {
    window.localStorage.setItem(localeStorageKey, "en-US");
    render(
      <I18nProvider>
        <Probe />
      </I18nProvider>,
    );
    expect(screen.getByTestId("nav")).toHaveTextContent("Primary navigation");
  });

  it("keeps document.documentElement.lang in sync", () => {
    render(
      <I18nProvider>
        <Probe />
      </I18nProvider>,
    );
    expect(document.documentElement.lang).toBe("zh-CN");

    fireEvent.click(screen.getByRole("button", { name: "to-en" }));
    expect(document.documentElement.lang).toBe("en-US");
  });
});
