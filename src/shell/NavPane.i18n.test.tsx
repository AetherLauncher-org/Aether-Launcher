import { FluentProvider, webLightTheme } from "@fluentui/react-components";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { I18nProvider } from "../i18n/I18nProvider";
import { AccountsContext } from "../state/accounts";
import type { AccountsApi } from "../state/accounts";
import { NavigationContext } from "../state/navigation";
import { NavPane } from "./NavPane";

const emptyAccounts: AccountsApi = {
  book: { accounts: [], activeId: null },
  active: null,
  add: () => undefined,
  remove: () => undefined,
  signInMicrosoft: () => undefined,
  signInYggdrasil: () => undefined,
  syncYggdrasil: () => undefined,
  select: () => undefined,
};

const offlineActive: AccountsApi = {
  ...emptyAccounts,
  active: {
    id: "offline-1",
    kind: "offline",
    name: "Steve",
    uuidMode: "standard",
    uuid: null,
  },
};

function renderNavPane(locale: "zh-CN" | "en-US", accounts: AccountsApi = emptyAccounts) {
  if (locale === "en-US") {
    window.localStorage.setItem("aether.locale", "en-US");
  }
  render(
    <FluentProvider theme={webLightTheme}>
      <I18nProvider>
        <NavigationContext.Provider value={{ current: "home", navigate: () => undefined }}>
          <AccountsContext.Provider value={accounts}>
            <NavPane version="0.1.0" />
          </AccountsContext.Provider>
        </NavigationContext.Provider>
      </I18nProvider>
    </FluentProvider>,
  );
}

describe("NavPane i18n", () => {
  it("shows English nav labels and account copy under en-US", () => {
    renderNavPane("en-US", offlineActive);

    expect(screen.getByRole("navigation")).toHaveAttribute("aria-label", "Primary navigation");
    expect(screen.getByRole("button", { name: "Home" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Instances" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Download" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Skins" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Settings" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Current account Steve" })).toBeInTheDocument();
    expect(screen.getByText("Offline account")).toBeInTheDocument();
    expect(screen.getByText("v0.1.0")).toBeInTheDocument();
  });

  it("shows Chinese labels under zh-CN (default)", () => {
    renderNavPane("zh-CN");

    expect(screen.getByRole("navigation")).toHaveAttribute("aria-label", "主要导航");
    expect(screen.getByRole("button", { name: "主页" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "皮肤库" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "还没有账户" })).toBeInTheDocument();
    expect(screen.getByText("添加账户")).toBeInTheDocument();
  });
});
