import { fireEvent, render, screen, within } from "@testing-library/react";
import { FluentProvider, webLightTheme } from "@fluentui/react-components";
import { afterEach, describe, expect, it, vi } from "vitest";
import { accountBookStorageKey } from "../lib/accounts";
import type { PageDefinition } from "../navigation";
import { AccountsProvider } from "../state/AccountsProvider";
import { NavigationContext } from "../state/navigation";
import { AccountsPage } from "./AccountsPage";

const page: PageDefinition = {
  id: "accounts",
  label: "账户",
  summary: "用哪个身份进入游戏",
  icon: <span />,
  activeIcon: <span />,
};

function renderAccounts() {
  return render(
    <FluentProvider theme={webLightTheme}>
      <NavigationContext.Provider value={{ current: "accounts", navigate: vi.fn() }}>
        <AccountsProvider>
          <AccountsPage page={page} />
        </AccountsProvider>
      </NavigationContext.Provider>
    </FluentProvider>,
  );
}

async function addAccount(name: string, mode?: "随机" | "自定义", customUuid?: string) {
  fireEvent.click(screen.getAllByRole("button", { name: "添加离线账户" })[0]);
  const dialog = await screen.findByRole("dialog");
  fireEvent.change(within(dialog).getByLabelText(/^用户名/), { target: { value: name } });
  if (mode !== undefined) {
    fireEvent.click(within(dialog).getByRole("radio", { name: new RegExp(mode) }));
  }
  if (customUuid !== undefined) {
    fireEvent.change(within(dialog).getByLabelText(/自定义 UUID/), { target: { value: customUuid } });
  }
  fireEvent.click(within(dialog).getByRole("button", { name: "添加" }));
  return dialog;
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("AccountsPage", () => {
  it("starts with three empty account groups and a way to add each kind", () => {
    const { container } = renderAccounts();

    expect(screen.getByRole("heading", { level: 1, name: "账户" })).toBeInTheDocument();
    for (const group of ["microsoft", "thirdparty", "offline"]) {
      const section = container.querySelector(`[data-account-group="${group}"]`) as HTMLElement;
      expect(section).not.toBeNull();
      expect(within(section).getByText("0 个")).toBeInTheDocument();
      expect(within(section).getByText(/^还没有/)).toBeInTheDocument();
    }
    expect(screen.getAllByRole("button", { name: "添加离线账户" }).length).toBeGreaterThan(0);
  });

  it("puts the header buttons in the order Microsoft, third-party, offline", () => {
    const { container } = renderAccounts();

    const names = Array.from(container.querySelectorAll(".page-header-action button")).map((button) =>
      button.textContent?.trim(),
    );

    expect(names).toEqual(["登录微软账户", "第三方登录", "添加离线账户"]);
  });

  it("lists the account groups in the same order", () => {
    renderAccounts();

    const headings = screen.getAllByRole("heading", { level: 2 }).map((heading) => heading.textContent);

    expect(headings).toEqual(["微软账户", "第三方账户", "离线账户"]);
  });

  it("shows the third-party button but does not let it do anything yet", () => {
    renderAccounts();

    const button = screen.getByRole("button", { name: "第三方登录" });

    expect(button).toHaveAttribute("aria-disabled", "true");
    fireEvent.click(button);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("files a new offline account under the offline group and nowhere else", async () => {
    const { container } = renderAccounts();

    await addAccount("Steve");

    const offline = container.querySelector('[data-account-group="offline"]') as HTMLElement;
    const microsoft = container.querySelector('[data-account-group="microsoft"]') as HTMLElement;
    expect(await within(offline).findByText("Steve")).toBeInTheDocument();
    expect(within(offline).getByText("1 个")).toBeInTheDocument();
    expect(within(microsoft).queryByText("Steve")).not.toBeInTheDocument();
  });

  it("asks for the username and the UUID format in a dialog instead of a bare input", async () => {
    renderAccounts();

    fireEvent.click(screen.getAllByRole("button", { name: "添加离线账户" })[0]);

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("添加离线账户")).toBeInTheDocument();
    expect(within(dialog).getByLabelText(/^用户名/)).toBeInTheDocument();
    expect(within(dialog).getByRole("radio", { name: /标准/ })).toBeChecked();
    expect(within(dialog).getByRole("radio", { name: /随机/ })).not.toBeChecked();
    expect(within(dialog).getByRole("radio", { name: /自定义/ })).not.toBeChecked();
  });

  it("adds a standard account, makes it current, and remembers it", async () => {
    renderAccounts();

    await addAccount("Steve");

    expect(await screen.findByText("Steve")).toBeInTheDocument();
    expect(screen.getByText("当前使用")).toBeInTheDocument();
    expect(screen.getByText(/由用户名生成/)).toBeInTheDocument();
    const stored = JSON.parse(window.localStorage.getItem(accountBookStorageKey) ?? "{}");
    expect(stored.accounts).toHaveLength(1);
    expect(stored.accounts[0]).toMatchObject({ name: "Steve", uuidMode: "standard", uuid: null });
  });

  it("explains a bad username and keeps the dialog open", async () => {
    renderAccounts();

    const dialog = await addAccount("a b");

    expect(await within(dialog).findByText(/只能使用字母、数字和下划线/)).toBeInTheDocument();
    expect(screen.queryByText("当前使用")).not.toBeInTheDocument();
  });

  it("stores and shows a random UUID", async () => {
    vi.spyOn(crypto, "randomUUID").mockReturnValue("01234567-89ab-cdef-0123-456789abcdef");
    renderAccounts();

    await addAccount("Alex", "随机");

    expect(await screen.findByText("01234567-89ab-cdef-0123-456789abcdef")).toBeInTheDocument();
    const stored = JSON.parse(window.localStorage.getItem(accountBookStorageKey) ?? "{}");
    expect(stored.accounts[0]).toMatchObject({ uuidMode: "random", uuid: "0123456789abcdef0123456789abcdef" });
  });

  it("validates a custom UUID before adding", async () => {
    renderAccounts();

    const dialog = await addAccount("Alex", "自定义", "nope");
    expect(await within(dialog).findByText(/32 位十六进制/)).toBeInTheDocument();

    fireEvent.change(within(dialog).getByLabelText(/自定义 UUID/), {
      target: { value: "01234567-89AB-CDEF-0123-456789ABCDEF" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "添加" }));

    expect(await screen.findByText("01234567-89ab-cdef-0123-456789abcdef")).toBeInTheDocument();
  });

  it("switches the current account and removes one after confirming", async () => {
    // 直接写入账户簿：jsdom 里关闭中的「添加账户」对话框不会播完退场动画，
    // 会一直挡在后面，这条用例要测的是切换和移除，不是添加
    window.localStorage.setItem(
      accountBookStorageKey,
      JSON.stringify({
        accounts: [
          { id: "offline-1", kind: "offline", name: "Steve", uuidMode: "standard", uuid: null },
          { id: "offline-2", kind: "offline", name: "Alex", uuidMode: "standard", uuid: null },
        ],
        activeId: "offline-1",
      }),
    );
    renderAccounts();

    // 点卡片本身就是使用这个账户，没有单独的「使用」按钮
    expect(screen.queryByRole("button", { name: "使用此账户" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "使用 Alex" }));
    const alexCard = screen.getByText("Alex").closest("li") as HTMLElement;
    expect(within(alexCard).getByText("当前使用")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "移除 Alex" }));
    const confirm = await screen.findByRole("dialog");
    expect(within(confirm).getByText("移除账户「Alex」？")).toBeInTheDocument();
    fireEvent.click(within(confirm).getByRole("button", { name: "移除" }));

    expect(screen.queryByText("Alex")).not.toBeInTheDocument();
    const steveCard = (await screen.findByText("Steve")).closest("li") as HTMLElement;
    expect(within(steveCard).getByText("当前使用")).toBeInTheDocument();
    expect(JSON.parse(window.localStorage.getItem(accountBookStorageKey) ?? "{}").activeId).toBe("offline-1");
  });

  it("marks the current account's card as pressed and leaves the other one unpressed", () => {
    window.localStorage.setItem(
      accountBookStorageKey,
      JSON.stringify({
        accounts: [
          { id: "offline-1", kind: "offline", name: "Steve", uuidMode: "standard", uuid: null },
          { id: "offline-2", kind: "offline", name: "Alex", uuidMode: "standard", uuid: null },
        ],
        activeId: "offline-1",
      }),
    );
    renderAccounts();

    expect(screen.getByRole("button", { name: "使用 Steve" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "使用 Alex" })).toHaveAttribute("aria-pressed", "false");
  });

  it("removes an account without also selecting it (the remove button is not inside the card button)", async () => {
    window.localStorage.setItem(
      accountBookStorageKey,
      JSON.stringify({
        accounts: [
          { id: "offline-1", kind: "offline", name: "Steve", uuidMode: "standard", uuid: null },
          { id: "offline-2", kind: "offline", name: "Alex", uuidMode: "standard", uuid: null },
        ],
        activeId: "offline-1",
      }),
    );
    renderAccounts();

    fireEvent.click(screen.getByRole("button", { name: "移除 Alex" }));
    await screen.findByRole("dialog");

    expect(JSON.parse(window.localStorage.getItem(accountBookStorageKey) ?? "{}").activeId).toBe("offline-1");
    expect(screen.getByRole("button", { name: "移除 Alex" }).closest(".account-card-main")).toBeNull();
  });

  it("says Microsoft sign-in needs the desktop app instead of a button that does nothing", async () => {
    renderAccounts();

    expect(await screen.findByText(/微软登录需要桌面版/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "登录微软账户" })).toBeDisabled();
  });
});
