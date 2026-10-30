import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { accountBookStorageKey } from "../lib/accounts";
import { loginMethodStorageKey } from "../lib/loginMethod";
import type { PageDefinition } from "../navigation";
import { pretendBrowser, pretendDesktop, renderWithState, routeInvoke } from "../test/support";
import type { Handlers } from "../test/support";
import { AccountsPage } from "./AccountsPage";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const page: PageDefinition = {
  id: "accounts",
  label: "账户",
  summary: "用哪个身份进入游戏",
  icon: <span />,
  activeIcon: <span />,
};

const notch = {
  id: "069a79f4-44e9-4726-a5be-fca90e38aaf5",
  displayName: "Notch",
  uuid: "069a79f444e94726a5befca90e38aaf5",
  xuid: "2535400000000001",
};

const code = { userCode: "ABCD1234", verificationUri: "https://microsoft.com/devicelogin", expiresInSecs: 900 };

let listeners: Map<string, (event: { payload: unknown }) => void>;

function emit(event: unknown) {
  act(() => {
    listeners.get("login://status")?.({ payload: event });
  });
}

function backend(overrides: Handlers = {}): Handlers {
  return {
    microsoft_login_available: () => true,
    list_microsoft_accounts: () => [],
    begin_microsoft_login: () => code,
    cancel_microsoft_login: () => false,
    open_microsoft_verification_page: () => null,
    remove_microsoft_account: () => null,
    ...overrides,
  };
}

function renderPage() {
  return renderWithState(<AccountsPage page={page} />, { page: "accounts" });
}

async function startLogin() {
  const button = await screen.findByRole("button", { name: "登录微软账户" });
  await waitFor(() => expect(button).toBeEnabled());
  fireEvent.click(button);
  return screen.findByRole("dialog");
}

beforeEach(() => {
  window.localStorage.clear();
  pretendDesktop();
  listeners = new Map();
  vi.mocked(listen).mockImplementation(async (name, handler) => {
    listeners.set(name, handler as (event: { payload: unknown }) => void);
    return () => {
      listeners.delete(name);
    };
  });
});

afterEach(() => {
  pretendBrowser();
  vi.mocked(invoke).mockReset();
  vi.mocked(listen).mockReset();
});

describe("Microsoft sign-in on the accounts page", () => {
  it("says it needs the desktop app in the browser preview", async () => {
    pretendBrowser();
    renderPage();

    expect(await screen.findByText(/微软登录需要桌面版/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "登录微软账户" })).toBeDisabled();
  });

  it("says so when this build has no Azure application id", async () => {
    routeInvoke(vi.mocked(invoke), backend({ microsoft_login_available: () => false }));
    renderPage();

    expect(await screen.findByText(/缺少 Azure 应用 ID/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "登录微软账户" })).toBeDisabled();
  });

  it("shows the code to type and signs the account in when the backend reports it", async () => {
    routeInvoke(vi.mocked(invoke), backend());
    renderPage();

    const dialog = await startLogin();
    expect(await within(dialog).findByText("ABCD1234")).toBeInTheDocument();
    expect(within(dialog).getByText(code.verificationUri)).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("begin_microsoft_login");

    emit({ state: "signed-in", account: notch });

    expect(await within(dialog).findByText("已登录：Notch")).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "完成" }));
    expect(await screen.findByText("Notch")).toBeInTheDocument();
    // 账户卡片上的徽章，加上「微软账户」小节标题旁边那个
    expect(screen.getAllByText("正版")).toHaveLength(2);
  });

  it("stores only the identity of the account, never a token", async () => {
    routeInvoke(vi.mocked(invoke), backend());
    renderPage();
    await startLogin();
    await screen.findByText("ABCD1234");

    emit({ state: "signed-in", account: notch });
    await screen.findByText("已登录：Notch");

    const stored = window.localStorage.getItem(accountBookStorageKey) ?? "";
    const book = JSON.parse(stored) as { accounts: Record<string, unknown>[]; activeId: string };
    expect(book.activeId).toBe(notch.id);
    expect(book.accounts[0]).toEqual({ id: notch.id, kind: "microsoft", name: "Notch", uuid: notch.uuid });
    expect(stored).not.toMatch(/token|refresh|access/i);
  });

  it("explains a failed sign-in in plain words and offers a retry", async () => {
    routeInvoke(vi.mocked(invoke), backend());
    renderPage();
    const dialog = await startLogin();
    await within(dialog).findByText("ABCD1234");

    emit({ state: "failed", failure: { reason: "child-account", message: "child" } });

    expect(await within(dialog).findByRole("alert")).toHaveTextContent("未成年人账户");
    expect(within(dialog).getByRole("button", { name: "重试" })).toBeInTheDocument();
  });

  it("asks for a new code when retrying", async () => {
    routeInvoke(vi.mocked(invoke), backend());
    renderPage();
    const dialog = await startLogin();
    await within(dialog).findByText("ABCD1234");
    emit({ state: "failed", failure: { reason: "expired", message: "expired" } });

    fireEvent.click(await within(dialog).findByRole("button", { name: "重试" }));

    await waitFor(() =>
      expect(vi.mocked(invoke).mock.calls.filter(([name]) => name === "begin_microsoft_login")).toHaveLength(2),
    );
  });

  it("shows the backend's refusal when no code could be requested", async () => {
    routeInvoke(
      vi.mocked(invoke),
      backend({
        begin_microsoft_login: () => {
          throw "microsoft-not-configured";
        },
      }),
    );
    renderPage();

    const dialog = await startLogin();

    expect(await within(dialog).findByRole("alert")).toHaveTextContent("还没有配置微软登录");
  });

  it("cancels the sign-in when the dialog is closed", async () => {
    routeInvoke(vi.mocked(invoke), backend());
    renderPage();
    const dialog = await startLogin();
    await within(dialog).findByText("ABCD1234");

    fireEvent.click(within(dialog).getByRole("button", { name: "取消" }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("cancel_microsoft_login"));
  });

  it("opens the verification page without sending any URL from the page", async () => {
    routeInvoke(vi.mocked(invoke), backend());
    renderPage();
    const dialog = await startLogin();
    await within(dialog).findByText("ABCD1234");

    fireEvent.click(within(dialog).getByRole("button", { name: "打开登录页面" }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("open_microsoft_verification_page"));
  });

  describe("with the browser sign-in chosen in Settings", () => {
    beforeEach(() => {
      window.localStorage.setItem(loginMethodStorageKey, "browser");
    });

    const browserBackend = (overrides: Handlers = {}) =>
      backend({ begin_microsoft_browser_login: () => ({ expiresInSecs: 600 }), ...overrides });

    it("opens the browser instead of showing a code", async () => {
      routeInvoke(vi.mocked(invoke), browserBackend());
      renderPage();

      const dialog = await startLogin();

      expect(await within(dialog).findByText(/已在浏览器里打开微软的登录页面/)).toBeInTheDocument();
      expect(invoke).toHaveBeenCalledWith("begin_microsoft_browser_login");
      expect(invoke).not.toHaveBeenCalledWith("begin_microsoft_login");
      expect(within(dialog).queryByLabelText("登录码")).not.toBeInTheDocument();
    });

    it("signs the account in when the backend reports it", async () => {
      routeInvoke(vi.mocked(invoke), browserBackend());
      renderPage();
      const dialog = await startLogin();
      await within(dialog).findByText(/已在浏览器里打开/);

      emit({ state: "signed-in", account: notch });

      expect(await within(dialog).findByText("已登录：Notch")).toBeInTheDocument();
    });

    it("reopens the sign-in page without sending any URL from the page", async () => {
      routeInvoke(vi.mocked(invoke), browserBackend());
      renderPage();
      const dialog = await startLogin();
      await within(dialog).findByText(/已在浏览器里打开/);

      fireEvent.click(within(dialog).getByRole("button", { name: "重新打开登录页面" }));

      await waitFor(() => expect(invoke).toHaveBeenCalledWith("open_microsoft_verification_page"));
    });

    it("offers the device code when the browser sign-in fails, for this attempt only", async () => {
      routeInvoke(vi.mocked(invoke), browserBackend());
      renderPage();
      const dialog = await startLogin();
      await within(dialog).findByText(/已在浏览器里打开/);
      emit({ state: "failed", failure: { reason: "service", message: "AADSTS50011" } });

      fireEvent.click(await within(dialog).findByRole("button", { name: "改用设备码重试" }));

      expect(await within(dialog).findByText("ABCD1234")).toBeInTheDocument();
      expect(invoke).toHaveBeenCalledWith("begin_microsoft_login");
      // 设置里保存的还是浏览器授权
      expect(window.localStorage.getItem(loginMethodStorageKey)).toBe("browser");

      fireEvent.click(within(dialog).getByRole("button", { name: "取消" }));
      await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
      vi.mocked(invoke).mockClear();

      const next = await startLogin();
      await within(next).findByText(/已在浏览器里打开/);
      expect(invoke).toHaveBeenCalledWith("begin_microsoft_browser_login");
    });

    it("retrying keeps the method that was used", async () => {
      routeInvoke(vi.mocked(invoke), browserBackend());
      renderPage();
      const dialog = await startLogin();
      await within(dialog).findByText(/已在浏览器里打开/);
      emit({ state: "failed", failure: { reason: "denied", message: "declined" } });

      fireEvent.click(await within(dialog).findByRole("button", { name: "重试" }));

      await waitFor(() =>
        expect(
          vi.mocked(invoke).mock.calls.filter(([name]) => name === "begin_microsoft_browser_login"),
        ).toHaveLength(2),
      );
    });

    it("cancels the sign-in when the dialog is closed", async () => {
      routeInvoke(vi.mocked(invoke), browserBackend());
      renderPage();
      const dialog = await startLogin();
      await within(dialog).findByText(/已在浏览器里打开/);

      fireEvent.click(within(dialog).getByRole("button", { name: "取消" }));

      await waitFor(() => expect(invoke).toHaveBeenCalledWith("cancel_microsoft_login"));
    });
  });

  it("brings the signed-in accounts back from the backend at startup", async () => {
    routeInvoke(vi.mocked(invoke), backend({ list_microsoft_accounts: () => [notch] }));
    renderPage();

    expect(await screen.findByText("Notch")).toBeInTheDocument();
    expect(screen.getAllByText("正版")).toHaveLength(2);
  });

  it("keeps the accounts it already has when the backend cannot be read", async () => {
    window.localStorage.setItem(
      accountBookStorageKey,
      JSON.stringify({
        accounts: [{ id: notch.id, kind: "microsoft", name: "Notch", uuid: notch.uuid }],
        activeId: notch.id,
      }),
    );
    routeInvoke(
      vi.mocked(invoke),
      backend({
        list_microsoft_accounts: () => {
          throw new Error("vault locked");
        },
      }),
    );
    renderPage();

    expect(await screen.findByText("Notch")).toBeInTheDocument();
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("list_microsoft_accounts"));
    expect(screen.getByText("Notch")).toBeInTheDocument();
  });

  it("drops an account the backend no longer knows about", async () => {
    window.localStorage.setItem(
      accountBookStorageKey,
      JSON.stringify({
        accounts: [{ id: notch.id, kind: "microsoft", name: "Notch", uuid: notch.uuid }],
        activeId: notch.id,
      }),
    );
    routeInvoke(vi.mocked(invoke), backend({ list_microsoft_accounts: () => [] }));
    renderPage();

    await waitFor(() => expect(screen.queryByText("Notch")).not.toBeInTheDocument());
  });

  it("signs the account out of the backend before removing it from the list", async () => {
    routeInvoke(vi.mocked(invoke), backend({ list_microsoft_accounts: () => [notch] }));
    renderPage();
    await screen.findByText("Notch");

    fireEvent.click(screen.getByRole("button", { name: "移除 Notch" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/系统凭据库里保存的登录/)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "移除" }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("remove_microsoft_account", { id: notch.id }),
    );
    await waitFor(() => expect(screen.queryByText("Notch")).not.toBeInTheDocument());
  });

  it("keeps the account listed when the backend could not sign it out", async () => {
    routeInvoke(
      vi.mocked(invoke),
      backend({
        list_microsoft_accounts: () => [notch],
        remove_microsoft_account: () => {
          throw "the account could not be removed from the system credential vault";
        },
      }),
    );
    renderPage();
    await screen.findByText("Notch");

    fireEvent.click(screen.getByRole("button", { name: "移除 Notch" }));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "移除" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent("system credential vault");
    expect(screen.getAllByText("Notch").length).toBeGreaterThan(0);
  });
});
