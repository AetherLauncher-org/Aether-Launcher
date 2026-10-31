import { FluentProvider, webLightTheme } from "@fluentui/react-components";
import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../i18n/I18nProvider";
import { localeStorageKey } from "../i18n/locale";
import { settingsPage } from "../navigation";
import { pretendBrowser, pretendDesktop, routeInvoke } from "../test/support";
import { SettingsPage } from "./SettingsPage";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

function renderSettings() {
  render(
    <FluentProvider theme={webLightTheme}>
      <I18nProvider>
        <SettingsPage
          page={settingsPage}
          themePreference="dark"
          onThemeChange={() => undefined}
          motion="full"
          onMotionChange={() => undefined}
          version="0.1.0"
        />
      </I18nProvider>
    </FluentProvider>,
  );
}

describe("SettingsPage language selector", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    pretendDesktop();
    routeInvoke(vi.mocked(invoke), {
      system_memory: () => ({ totalMib: 16_284, availableMib: 2_355 }),
      list_java_runtimes: () => [],
    });
  });

  afterEach(() => {
    pretendBrowser();
    window.localStorage.clear();
  });

  it("offers the language choice and defaults to 简体中文", () => {
    renderSettings();

    expect(screen.getByText("界面语言")).toBeInTheDocument();
    const select = screen.getByRole("combobox", { name: "界面语言" });
    expect(select).toHaveValue("zh-CN");
    expect(screen.getByRole("option", { name: "简体中文" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "English" })).toBeInTheDocument();
  });

  it("switches the language row (and interface) to English and persists", () => {
    renderSettings();

    fireEvent.change(screen.getByRole("combobox", { name: "界面语言" }), {
      target: { value: "en-US" },
    });

    expect(window.localStorage.getItem(localeStorageKey)).toBe("en-US");
    expect(screen.getByText("Language")).toBeInTheDocument();
  });
});
