import { invoke } from "@tauri-apps/api/core";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { instanceSettingsKey } from "../lib/instanceSettings";
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

const survival = { id: "i1", name: "Survival", version: "1.21.1", state: "ready" };
const creative = { id: "i2", name: "Creative", version: "1.20.1", state: "ready" };

const details = {
  ...survival,
  loader: "Fabric 0.16.5",
  sizeBytes: 5 * 1024 * 1024,
  counts: { mods: 2, resourcepacks: 1, shaderpacks: 0, saves: 1, schematics: 0 },
};

const mods = [
  { name: "sodium-0.6.jar", sizeBytes: 1_300_000, enabled: true, isFolder: false },
  { name: "lithium-0.14.jar.disabled", sizeBytes: 900_000, enabled: false, isFolder: false },
];

type Calls = [string, unknown][];

function calls(name: string): Calls {
  return vi.mocked(invoke).mock.calls.filter(([command]) => command === name) as Calls;
}

function backend(overrides: Handlers = {}, listing: unknown[] = [survival, creative]) {
  routeInvoke(vi.mocked(invoke), {
    list_instances: () => ({ instances: listing, skipped: [] }),
    instance_details: () => details,
    list_instance_content: (args) => ((args as { kind: string }).kind === "mods" ? mods : []),
    list_instance_logs: () => [{ kind: "latest", name: "latest.log", sizeBytes: 100, modified: null }],
    read_instance_log: () => ({ text: "Setting user: <USERNAME:1>", truncated: false }),
    system_memory: () => ({ totalMib: 16_384, availableMib: 8192 }),
    set_instance_mod_enabled: () => null,
    remove_instance_content: () => null,
    open_instance_folder: () => null,
    rename_installed_instance: (args) => ({ ...survival, name: (args as { name: string }).name }),
    delete_installed_instance: () => null,
    ...overrides,
  });
}

async function openManagement(name = "Survival") {
  renderWithState(<InstancesPage page={page} />);
  fireEvent.click(await screen.findByRole("button", { name: `管理 ${name}` }));
  return screen.findByRole("heading", { level: 1, name });
}

beforeEach(() => {
  window.localStorage.clear();
  vi.mocked(invoke).mockReset();
  pretendDesktop();
});

afterEach(() => {
  pretendBrowser();
});

describe("instance management page", () => {
  it("opens from a card, shows the loader and the overview, and goes back to the list", async () => {
    backend();
    await openManagement();

    // 标题旁的徽章和右侧信息栏各显示一次
    expect(await screen.findAllByText("Fabric 0.16.5")).toHaveLength(2);
    expect(calls("instance_details")[0][1]).toEqual({ id: "i1" });
    expect(screen.getByRole("tab", { name: "概览" })).toHaveAttribute("aria-selected", "true");
    // 占用空间来自后端的字节数
    expect(await screen.findByText("5.0 MB")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "返回实例列表" }));

    expect(await screen.findByRole("button", { name: "管理 Creative" })).toBeInTheDocument();
  });

  it("lists mods by their names, with the switched-off one marked", async () => {
    backend();
    await openManagement();

    fireEvent.click(await screen.findByRole("tab", { name: /^模组/ }));

    expect(await screen.findByText("sodium-0.6")).toBeInTheDocument();
    expect(screen.getByText("lithium-0.14")).toBeInTheDocument();
    expect(screen.getByText("已关闭")).toBeInTheDocument();
    expect(calls("list_instance_content").some(([, args]) => (args as { kind: string }).kind === "mods")).toBe(true);
  });

  it("switches a mod with its plain file name, never a path", async () => {
    backend();
    await openManagement();
    fireEvent.click(await screen.findByRole("tab", { name: /^模组/ }));

    fireEvent.click(await screen.findByRole("switch", { name: "启用 lithium-0.14" }));

    await waitFor(() => expect(calls("set_instance_mod_enabled")).toHaveLength(1));
    expect(calls("set_instance_mod_enabled")[0][1]).toEqual({
      id: "i1",
      name: "lithium-0.14.jar.disabled",
      enabled: true,
    });
  });

  it("asks before deleting a file, and does nothing when cancelled", async () => {
    backend();
    await openManagement();
    fireEvent.click(await screen.findByRole("tab", { name: /^模组/ }));

    fireEvent.click(await screen.findByRole("button", { name: "删除 sodium-0.6" }));
    const dialog = await screen.findByRole("alertdialog");
    expect(within(dialog).getByText(/「sodium-0.6」会从实例里删除，无法恢复/)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "取消" }));

    expect(calls("remove_instance_content")).toHaveLength(0);

    fireEvent.click(screen.getByRole("button", { name: "删除 sodium-0.6" }));
    fireEvent.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "删除" }));

    await waitFor(() => expect(calls("remove_instance_content")).toHaveLength(1));
    expect(calls("remove_instance_content")[0][1]).toEqual({ id: "i1", kind: "mods", name: "sodium-0.6.jar" });
  });

  it("shows the backend's refusal when a file cannot be deleted", async () => {
    backend({
      remove_instance_content: () => {
        throw "stop the game before deleting its files";
      },
    });
    await openManagement();
    fireEvent.click(await screen.findByRole("tab", { name: /^模组/ }));

    fireEvent.click(await screen.findByRole("button", { name: "删除 sodium-0.6" }));
    fireEvent.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "删除" }));

    expect(await within(await screen.findByRole("alertdialog")).findByRole("alert")).toHaveTextContent(
      "stop the game before deleting its files",
    );
  });

  it("shows an empty folder with a way to open it", async () => {
    backend();
    await openManagement();

    fireEvent.click(await screen.findByRole("tab", { name: /^光影/ }));

    expect(await screen.findByText("还没有光影包")).toBeInTheDocument();
    fireEvent.click(screen.getAllByRole("button", { name: "打开光影文件夹" })[0]);
    await waitFor(() => expect(calls("open_instance_folder")).toHaveLength(1));
    expect(calls("open_instance_folder")[0][1]).toEqual({ id: "i1", target: "shaderpacks" });
  });

  it("opens the game folder by label only", async () => {
    backend();
    await openManagement();

    fireEvent.click(await screen.findByRole("button", { name: "打开游戏文件夹" }));

    await waitFor(() => expect(calls("open_instance_folder")).toHaveLength(1));
    expect(calls("open_instance_folder")[0][1]).toEqual({ id: "i1", target: "game" });
  });

  it("renames the instance with the trimmed name and refreshes the list", async () => {
    backend();
    await openManagement();

    fireEvent.click(await screen.findByRole("button", { name: "重命名" }));
    const dialog = await screen.findByRole("dialog");
    fireEvent.change(within(dialog).getByRole("textbox"), { target: { value: "  Hardcore  " } });
    fireEvent.click(within(dialog).getByRole("button", { name: "保存" }));

    await waitFor(() => expect(calls("rename_installed_instance")).toHaveLength(1));
    expect(calls("rename_installed_instance")[0][1]).toEqual({ id: "i1", name: "Hardcore" });
    await waitFor(() => expect(calls("list_instances").length).toBeGreaterThan(1));
  });

  it("does not rename to a name another instance already has", async () => {
    backend();
    await openManagement();

    fireEvent.click(await screen.findByRole("button", { name: "重命名" }));
    const dialog = await screen.findByRole("dialog");
    fireEvent.change(within(dialog).getByRole("textbox"), { target: { value: "creative" } });

    expect(await within(dialog).findByText("已经有同名实例了")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "保存" })).toBeDisabled();
    expect(calls("rename_installed_instance")).toHaveLength(0);
  });

  it("deletes the instance after confirmation, clears its settings and returns to the list", async () => {
    window.localStorage.setItem(instanceSettingsKey("i1"), JSON.stringify({ memoryMiB: 4096 }));
    let remaining: unknown[] = [survival, creative];
    backend({
      list_instances: () => ({ instances: remaining, skipped: [] }),
      delete_installed_instance: () => {
        remaining = [creative];
        return null;
      },
    });
    await openManagement();

    fireEvent.click(await screen.findByRole("button", { name: "删除实例" }));
    const dialog = await screen.findByRole("alertdialog");
    expect(within(dialog).getByText(/存档、模组、资源包、截图和设置都会被删除/)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "删除实例" }));

    await waitFor(() => expect(calls("delete_installed_instance")).toHaveLength(1));
    expect(calls("delete_installed_instance")[0][1]).toEqual({ id: "i1" });
    // 回到列表后重新读取，已删除的实例不再出现
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "管理 Survival" })).not.toBeInTheDocument(),
    );
    expect(await screen.findByRole("button", { name: "管理 Creative" })).toBeInTheDocument();
    expect(window.localStorage.getItem(instanceSettingsKey("i1"))).toBeNull();
  });

  it("shows the redacted log text the backend sent", async () => {
    backend();
    await openManagement();

    fireEvent.click(await screen.findByRole("tab", { name: "日志" }));

    expect(await screen.findByLabelText("日志内容")).toHaveTextContent("Setting user: <USERNAME:1>");
    expect(screen.getByText(/玩家名、令牌、邮箱、IP 地址和用户目录已被替换成占位符/)).toBeInTheDocument();
    expect(calls("read_instance_log")[0][1]).toEqual({ id: "i1", kind: "latest", name: null });
  });

  it("says when a long log was cut", async () => {
    backend({ read_instance_log: () => ({ text: "tail", truncated: true }) });
    await openManagement();

    fireEvent.click(await screen.findByRole("tab", { name: "日志" }));

    expect(await screen.findByText("日志很长，只显示最后 256 KB。")).toBeInTheDocument();
  });

  it("asks a crash report by name", async () => {
    backend({
      list_instance_logs: () => [
        { kind: "latest", name: "latest.log", sizeBytes: 1, modified: null },
        { kind: "crash", name: "crash-1.txt", sizeBytes: 2, modified: 1_700_000_000 },
      ],
    });
    await openManagement();
    fireEvent.click(await screen.findByRole("tab", { name: "日志" }));

    const select = await screen.findByRole("combobox", { name: "选择日志" });
    fireEvent.change(select, { target: { value: "crash:crash-1.txt" } });

    await waitFor(() =>
      expect(calls("read_instance_log").some(([, args]) => (args as { kind: string }).kind === "crash")).toBe(true),
    );
    const crash = calls("read_instance_log").find(([, args]) => (args as { kind: string }).kind === "crash");
    expect(crash?.[1]).toEqual({ id: "i1", kind: "crash", name: "crash-1.txt" });
  });

  it("keeps a memory choice for this instance only, on this computer", async () => {
    backend();
    await openManagement();

    fireEvent.click(await screen.findByRole("radio", { name: "只为这个实例设置" }));

    const saved = JSON.parse(window.localStorage.getItem(instanceSettingsKey("i1")) ?? "{}") as {
      memoryMiB: number | null;
    };
    expect(saved.memoryMiB).toBeGreaterThan(0);
    expect(window.localStorage.getItem(instanceSettingsKey("i2"))).toBeNull();

    fireEvent.click(screen.getByRole("radio", { name: "跟随全局设置" }));
    expect(window.localStorage.getItem(instanceSettingsKey("i1"))).toBeNull();
  });

  it("reports an instance it cannot read, with a retry", async () => {
    backend({
      instance_details: () => {
        throw "cannot open the instance: nope";
      },
    });
    await openManagement();

    expect(await screen.findByText("读取实例失败")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重新读取" })).toBeInTheDocument();
  });
});
