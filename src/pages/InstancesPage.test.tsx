import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { InstanceLaunchPanel } from "../components/InstanceLaunchPanel";
import { InstancesPage } from "./InstancesPage";
import type { PageDefinition } from "../navigation";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const page: PageDefinition = {
  id: "instances",
  label: "实例",
  icon: <span />,
  summary: "每个实例的版本、模组、Java 与启动参数相互隔离",
  pendingTitle: "",
  pendingBody: "",
};

function enableTauri() {
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {},
    configurable: true,
  });
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

afterEach(() => {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
});

describe("InstancesPage", () => {
  it("renders the empty state in a plain browser", () => {
    render(<InstancesPage page={page} />);

    expect(screen.getByRole("heading", { level: 1, name: "实例" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "还没有实例" })).toBeInTheDocument();
    expect(screen.getByText(/浏览器预览读不到本机实例/)).toBeInTheDocument();
  });

  it("explains that importing is not available yet", () => {
    render(<InstancesPage page={page} />);

    expect(screen.getByRole("button", { name: "导入实例" })).toBeDisabled();
  });

  it("lists instances returned by the backend", async () => {
    enableTauri();
    vi.mocked(invoke).mockResolvedValue({
      instances: [
        { id: "a", name: "生存 1.21", version: "1.21.1", state: "ready" },
        { id: "b", name: "红石实验", version: "1.20.1", state: "running" },
      ],
      skipped: [],
    });

    render(<InstancesPage page={page} />);

    await waitFor(() =>
      expect(screen.getByText("生存 1.21")).toBeInTheDocument(),
    );
    expect(screen.getByText("红石实验")).toBeInTheDocument();
    expect(screen.getByText("就绪")).toBeInTheDocument();
    expect(screen.getByText("运行中")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("list_instances");
  });

  it("shows an error state with a retry button", async () => {
    enableTauri();
    vi.mocked(invoke).mockRejectedValue("实例目录不可读");

    render(<InstancesPage page={page} />);

    await waitFor(() =>
      expect(screen.getByRole("heading", { name: "读取实例失败" })).toBeInTheDocument(),
    );
    expect(screen.getByText("实例目录不可读")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重新读取" })).toBeInTheDocument();
  });

  it("reports records the backend refused to load", async () => {
    enableTauri();
    vi.mocked(invoke).mockResolvedValue({
      instances: [],
      skipped: [{ id: "broken", reason: "invalid-json" }],
    });

    render(<InstancesPage page={page} />);

    await waitFor(() =>
      expect(screen.getByText(/有 1 条记录没读出来/)).toBeInTheDocument(),
    );
    expect(screen.getByText(/实例文件格式损坏/)).toBeInTheDocument();
  });

  it("filters the list by name", async () => {
    enableTauri();
    vi.mocked(invoke).mockResolvedValue({
      instances: [
        { id: "a", name: "生存 1.21", version: "1.21.1", state: "ready" },
        { id: "b", name: "红石实验", version: "1.20.1", state: "ready" },
      ],
      skipped: [],
    });

    const { findByLabelText, findByText } = render(
      <InstancesPage page={page} />,
    );

    const search = await findByLabelText("筛选实例");
    const { fireEvent } = await import("@testing-library/react");
    fireEvent.change(search, { target: { value: "红石" } });

    expect(await findByText("红石实验")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.queryByText("生存 1.21")).not.toBeInTheDocument(),
    );
  });

  it("mounts a launch panel per listed instance", async () => {
    enableTauri();
    vi.mocked(invoke).mockResolvedValue({
      instances: [
        { id: "a", name: "生存 1.21", version: "1.21.1", state: "ready" },
        { id: "b", name: "红石实验", version: "1.20.1", state: "ready" },
      ],
      skipped: [],
    });

    render(<InstancesPage page={page} />);

    await waitFor(() =>
      expect(screen.getByText("生存 1.21")).toBeInTheDocument(),
    );
    expect(await screen.findAllByText("未验证")).toHaveLength(2);
  });

  it("exposes exactly one launch control per instance on the whole page", async () => {
    enableTauri();
    vi.mocked(invoke).mockResolvedValue({
      instances: [{ id: "a", name: "生存 1.21", version: "1.21.1", state: "ready" }],
      skipped: [],
    });

    render(<InstancesPage page={page} />);

    await waitFor(() =>
      expect(screen.getByText("生存 1.21")).toBeInTheDocument(),
    );
    expect(screen.getAllByRole("button", { name: "启动" })).toHaveLength(1);
  });

  it("shows the cautious initial label for a running instance", async () => {
    enableTauri();
    vi.mocked(invoke).mockResolvedValue({
      instances: [
        { id: "a", name: "生存 1.21", version: "1.21.1", state: "running" },
      ],
      skipped: [],
    });

    render(<InstancesPage page={page} />);

    expect(
      await screen.findByText("运行中（未验证）"),
    ).toBeInTheDocument();
  });

  it("starts through the single launch control with exactly one IPC pair", async () => {
    enableTauri();
    mockCommands({
      list_instances: {
        instances: [
          { id: "a", name: "生存 1.21", version: "1.21.1", state: "ready" },
        ],
        skipped: [],
      },
      activate_installed_instance: undefined,
      start_installed_instance: runningStatus,
    });

    render(<InstancesPage page={page} />);
    const start = await screen.findByRole("button", { name: "启动" });
    fireEvent.click(start);

    await waitFor(() => expect(screen.getByText("运行中")).toBeInTheDocument());
    const calls = vi.mocked(invoke).mock.calls.filter(
      ([command]) => command === "activate_installed_instance",
    );
    expect(calls).toHaveLength(1);
    expect(invoke).toHaveBeenCalledWith("activate_installed_instance", {
      id: "a",
    });
    expect(invoke).toHaveBeenCalledWith("start_installed_instance", {
      id: "a",
      offlineName: "Player",
      memoryMib: 4096,
    });
  });

  it("ignores repeated clicks while a launch is in flight", async () => {
    enableTauri();
    let resolveActivate: (() => void) | undefined;
    vi.mocked(invoke).mockImplementation(((cmd: string) => {
      if (cmd === "list_instances") {
        return Promise.resolve({
          instances: [
            { id: "a", name: "生存 1.21", version: "1.21.1", state: "ready" },
          ],
          skipped: [],
        });
      }
      if (cmd === "activate_installed_instance") {
        return new Promise<void>((resolve) => {
          resolveActivate = resolve;
        });
      }
      return Promise.resolve(runningStatus);
    }) as unknown as typeof invoke);

    render(<InstancesPage page={page} />);
    const start = await screen.findByRole("button", { name: "启动" });
    fireEvent.click(start);
    fireEvent.click(start);
    fireEvent.click(start);

    expect(
      vi.mocked(invoke).mock.calls.filter(
        ([command]) => command === "activate_installed_instance",
      ),
    ).toHaveLength(1);

    resolveActivate?.();
    await waitFor(() =>
      expect(
        vi.mocked(invoke).mock.calls.filter(
          ([command]) => command === "start_installed_instance",
        ),
      ).toHaveLength(1),
    );
  });

  it("no longer renders inline start/stop buttons in the list rows", async () => {
    enableTauri();
    vi.mocked(invoke).mockResolvedValue({
      instances: [
        { id: "a", name: "生存 1.21", version: "1.21.1", state: "running" },
        { id: "b", name: "红石实验", version: "1.20.1", state: "ready" },
      ],
      skipped: [],
    });

    render(<InstancesPage page={page} />);

    await waitFor(() =>
      expect(screen.getByText("生存 1.21")).toBeInTheDocument(),
    );
    expect(
      screen.queryByRole("button", { name: "停止 生存 1.21" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "启动 红石实验" }),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("处理中")).not.toBeInTheDocument();
  });
});

/** 按命令名分发 mock 返回值；值是 Error 时按失败处理。 */
function mockCommands(commands: Record<string, unknown>) {
  vi.mocked(invoke).mockImplementation(((cmd: string) => {
    const result = commands[cmd];
    if (result instanceof Error) {
      return Promise.reject(result.message);
    }
    return Promise.resolve(result);
  }) as unknown as typeof invoke);
}

const runningStatus = {
  instanceId: "a",
  state: "running",
  pid: 4321,
  exitCode: null,
  discardedLogs: 0,
} as const;

const stoppedStatus = {
  instanceId: "a",
  state: "stopped",
  pid: 0,
  exitCode: 0,
  discardedLogs: 0,
} as const;

describe("InstanceLaunchPanel", () => {
  it("shows a desktop-only notice outside Tauri", () => {
    render(<InstanceLaunchPanel instanceId="a" />);

    expect(screen.getByText("需要桌面版才能启动实例")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "启动" })).not.toBeInTheDocument();
  });

  it("disables starting while the player name is invalid", () => {
    enableTauri();

    render(<InstanceLaunchPanel instanceId="a" />);

    fireEvent.change(screen.getByLabelText("离线玩家名"), {
      target: { value: "ab" },
    });

    expect(screen.getByRole("button", { name: "启动" })).toBeDisabled();
    expect(screen.getByText("玩家名至少 3 个字符")).toBeInTheDocument();
  });

  it("reports an activation failure as missing files", async () => {
    enableTauri();
    mockCommands({
      activate_installed_instance: new Error("版本文件缺失"),
    });

    render(<InstanceLaunchPanel instanceId="a" />);
    fireEvent.click(screen.getByRole("button", { name: "启动" }));

    expect(await screen.findByText("缺文件")).toBeInTheDocument();
    expect(screen.getByText("版本文件缺失")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重试" })).toBeInTheDocument();
  });

  it("starts, shows the PID, and stops with the right command arguments", async () => {
    enableTauri();
    mockCommands({
      activate_installed_instance: undefined,
      start_installed_instance: runningStatus,
      stop_installed_instance: stoppedStatus,
    });

    render(<InstanceLaunchPanel instanceId="a" />);
    fireEvent.click(screen.getByRole("button", { name: "启动" }));

    expect(await screen.findByText("运行中")).toBeInTheDocument();
    expect(screen.getByText("PID 4321")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("activate_installed_instance", {
      id: "a",
    });
    expect(invoke).toHaveBeenCalledWith("start_installed_instance", {
      id: "a",
      offlineName: "Player",
      memoryMib: 4096,
    });

    fireEvent.click(screen.getByRole("button", { name: "停止" }));

    expect(await screen.findByText("已停止")).toBeInTheDocument();
    expect(screen.getByText("退出码 0")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("stop_installed_instance", {
      id: "a",
    });
  });
});
