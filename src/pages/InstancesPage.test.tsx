import { invoke } from "@tauri-apps/api/core";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { accountBookStorageKey } from "../lib/accounts";
import type { PageDefinition } from "../navigation";
import { pretendBrowser, pretendDesktop, renderWithState, routeInvoke } from "../test/support";
import type { Handlers } from "../test/support";
import { InstancesPage } from "./InstancesPage";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));

const page: PageDefinition = {
  id: "instances",
  label: "实例",
  summary: "每个实例的版本与存档相互独立",
  icon: <span />,
  activeIcon: <span />,
};

const ready = { id: "i1", name: "Survival", version: "1.21.1", state: "ready" };
const incomplete = { id: "i2", name: "Old Save", version: "1.20.1", state: "incomplete" };

const running = { instanceId: "i1", state: "running", pid: 4242, exitCode: null, discardedLogs: 0 };

function seedAccount(uuid: string | null = null) {
  window.localStorage.setItem(
    accountBookStorageKey,
    JSON.stringify({
      accounts: [
        {
          id: "offline-1",
          kind: "offline",
          name: "Steve",
          uuidMode: uuid === null ? "standard" : "custom",
          uuid,
        },
      ],
      activeId: "offline-1",
    }),
  );
}

function backend(overrides: Handlers = {}, instances: unknown[] = [ready, incomplete]) {
  routeInvoke(vi.mocked(invoke), {
    list_instances: () => ({ instances, skipped: [] }),
    ...overrides,
  });
}

/** 某个实例所在的卡片；页面上每张卡片都有自己的启动按钮。 */
function cardOf(name: string): HTMLElement {
  return screen.getByText(name).closest("li") as HTMLElement;
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  pretendDesktop();
});

afterEach(() => {
  pretendBrowser();
});

describe("InstancesPage", () => {
  it("lists the installed instances as cards with their version and memory setting", async () => {
    backend();
    renderWithState(<InstancesPage page={page} />);

    expect(await screen.findByText("Survival")).toBeInTheDocument();
    expect(screen.getByText("Old Save")).toBeInTheDocument();
    expect(screen.getByText("Vanilla 1.21.1 · 内存 自动")).toBeInTheDocument();
    expect(screen.getByText("共 2 个实例")).toBeInTheDocument();
  });

  it("does not offer to create an empty instance; installing a version creates one", async () => {
    backend();
    const { navigate } = renderWithState(<InstancesPage page={page} />);
    await screen.findByText("Survival");

    expect(screen.queryByRole("button", { name: /创建实例/ })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "安装新版本" }));

    expect(navigate).toHaveBeenCalledWith("download");
  });

  it("opens the import dialog from the page header", async () => {
    backend();
    renderWithState(<InstancesPage page={page} />);
    await screen.findByText("Survival");

    fireEvent.click(screen.getByRole("button", { name: "导入实例" }));

    expect(await screen.findByRole("dialog")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "从文件夹导入" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "从启动器导入" })).toBeInTheDocument();
  });

  it("asks for an account before anything can launch", async () => {
    backend();
    const { navigate } = renderWithState(<InstancesPage page={page} />);
    await screen.findByText("Survival");

    expect(screen.getByText(/还没有账户/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "去添加" }));
    expect(navigate).toHaveBeenCalledWith("accounts");

    // 没有账户时，可启动实例上的按钮引导去添加，而不是一个灰按钮
    fireEvent.click(screen.getByRole("button", { name: "先添加账户" }));
    expect(navigate).toHaveBeenCalledTimes(2);
  });

  it("launches with the current account and the global memory setting", async () => {
    seedAccount("0123456789abcdef0123456789abcdef");
    window.localStorage.setItem("aether.memoryMiB", "8192");
    backend({ start_installed_instance: () => running });
    renderWithState(<InstancesPage page={page} />);
    await screen.findByText("Survival");

    fireEvent.click(within(cardOf("Survival")).getByRole("button", { name: "启动" }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("start_installed_instance", {
        id: "i1",
        offlineName: "Steve",
        memoryMib: 8192,
        accountUuid: "0123456789abcdef0123456789abcdef",
        accountId: null,
      }),
    );
    expect(await screen.findByText("运行中")).toBeInTheDocument();
    expect(window.localStorage.getItem("aether.lastPlayed")).toBe("i1");
  });

  it("sizes the heap from the memory that is free at launch when memory is automatic", async () => {
    seedAccount();
    backend({
      start_installed_instance: () => running,
      system_memory: () => ({ totalMib: 16_384, availableMib: 10_240 }),
    });
    renderWithState(<InstancesPage page={page} />);
    await screen.findByText("Survival");

    fireEvent.click(within(cardOf("Survival")).getByRole("button", { name: "启动" }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "start_installed_instance",
        expect.objectContaining({ id: "i1", memoryMib: 6656 }),
      ),
    );
  });

  it("shows a custom memory value on the cards", async () => {
    window.localStorage.setItem("aether.memoryMode", "custom");
    window.localStorage.setItem("aether.memoryMiB", "6144");
    backend();
    renderWithState(<InstancesPage page={page} />);

    expect(await screen.findByText("Vanilla 1.21.1 · 内存 6 GiB")).toBeInTheDocument();
  });

  it("lets the backend derive the UUID for a standard account", async () => {
    seedAccount(null);
    backend({ start_installed_instance: () => running });
    renderWithState(<InstancesPage page={page} />);
    await screen.findByText("Survival");

    fireEvent.click(within(cardOf("Survival")).getByRole("button", { name: "启动" }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "start_installed_instance",
        expect.objectContaining({ offlineName: "Steve", accountUuid: null }),
      ),
    );
  });

  it("stops a running game", async () => {
    seedAccount();
    backend({
      start_installed_instance: () => running,
      stop_installed_instance: () => ({ ...running, state: "stopped" }),
    });
    renderWithState(<InstancesPage page={page} />);
    await screen.findByText("Survival");

    fireEvent.click(within(cardOf("Survival")).getByRole("button", { name: "启动" }));
    fireEvent.click(await within(cardOf("Survival")).findByRole("button", { name: "停止" }));

    expect(await screen.findByText("已停止")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("stop_installed_instance", { id: "i1" });
  });

  it("shows why a launch failed and offers a retry", async () => {
    seedAccount();
    backend({
      start_installed_instance: () => {
        throw "no suitable Java runtime was found";
      },
    });
    renderWithState(<InstancesPage page={page} />);
    await screen.findByText("Survival");

    fireEvent.click(within(cardOf("Survival")).getByRole("button", { name: "启动" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("no suitable Java runtime was found");
    expect(screen.getByText("启动失败")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重试" })).toBeEnabled();
  });

  it("does not let an incomplete instance launch and says what to do", async () => {
    seedAccount();
    backend();
    renderWithState(<InstancesPage page={page} />);
    await screen.findByText("Old Save");

    expect(screen.getByText("待补齐")).toBeInTheDocument();
    const buttons = screen.getAllByRole("button", { name: "启动" });
    expect(buttons).toHaveLength(2);
    expect(buttons.some((button) => button.hasAttribute("aria-disabled") || button.hasAttribute("disabled"))).toBe(
      true,
    );
  });

  it("filters by name or version", async () => {
    seedAccount();
    backend();
    renderWithState(<InstancesPage page={page} />);
    await screen.findByText("Survival");

    fireEvent.change(screen.getByLabelText("筛选实例"), { target: { value: "1.20" } });

    expect(screen.queryByText("Survival")).not.toBeInTheDocument();
    expect(screen.getByText("Old Save")).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("筛选实例"), { target: { value: "zzz" } });
    expect(screen.getByRole("heading", { name: "没有匹配的实例" })).toBeInTheDocument();
  });

  it("points an empty library at the download page", async () => {
    backend({}, []);
    const { navigate } = renderWithState(<InstancesPage page={page} />);

    expect(await screen.findByRole("heading", { name: "还没有实例" })).toBeInTheDocument();
    fireEvent.click(screen.getAllByRole("button", { name: "安装新版本" })[1]);

    expect(navigate).toHaveBeenCalledWith("download");
  });

  it("reports records that could not be read", async () => {
    routeInvoke(vi.mocked(invoke), {
      list_instances: () => ({ instances: [ready], skipped: [{ id: "bad", reason: "invalid-json" }] }),
    });
    renderWithState(<InstancesPage page={page} />);

    expect(await screen.findByText("有 1 条记录没读出来")).toBeInTheDocument();
    expect(screen.getByText(/实例文件格式损坏/)).toBeInTheDocument();
  });

  it("shows a retryable error when the listing fails", async () => {
    routeInvoke(vi.mocked(invoke), {
      list_instances: () => {
        throw "instance store unavailable";
      },
    });
    renderWithState(<InstancesPage page={page} />);

    expect(await screen.findByRole("alert")).toHaveTextContent("instance store unavailable");
    expect(screen.getByRole("button", { name: "重新读取" })).toBeInTheDocument();
  });

  it("explains that the browser preview cannot read local instances", () => {
    pretendBrowser();
    renderWithState(<InstancesPage page={page} />);

    expect(screen.getByText(/浏览器预览读不到本机实例/)).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalled();
  });
});
