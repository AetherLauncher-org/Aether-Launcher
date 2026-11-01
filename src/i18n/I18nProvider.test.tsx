import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { I18nProvider } from "./I18nProvider";
import { useT } from "./context";
import { localeStorageKey } from "./locale";

function Probe() {
  const { locale, setLocale, t } = useT();
  return (
    <div>
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

  it("translates with the default zh-CN when not overridden", () => {
    render(
      <I18nProvider>
        <Probe />
      </I18nProvider>,
    );
    expect(screen.getByTestId("nav")).toHaveTextContent("主要导航");
    expect(screen.getByTestId("install")).toHaveTextContent("安装 1.21.1 · 17%");
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
