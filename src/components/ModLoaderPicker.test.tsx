import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  InstanceSummary,
  ModLoaderChoice,
  ModLoaderInstallResult,
} from "../ipc";
import { pretendBrowser, pretendDesktop, renderWithState } from "../test/support";
import { ModLoaderPicker } from "./ModLoaderPicker";

// renderWithState 里的 InstallProvider 会订阅安装进度事件；桌面模式下它会真的去调
// Tauri 的 listen，这里换成空实现，和 DownloadPage.test.tsx 一样。
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));

// 直接 mock IPC 封装：这个组件只关心「读到了什么、装成功了没有」，
// 命令名与参数已经由 src/ipc/modloader.ts 的类型保证。
vi.mock("../ipc", async (importOriginal) => {
  const original = await importOriginal<typeof import("../ipc")>();
  return {
    ...original,
    listModLoaderVersions: vi.fn(),
    installModLoaderVersion: vi.fn(),
  };
});

const { listModLoaderVersions, installModLoaderVersion } = await import("../ipc");
const listVersions = vi.mocked(listModLoaderVersions);
const installLoader = vi.mocked(installModLoaderVersion);

/** 四个加载器，故意不按固定顺序返回；Forge 和 Quilt 不可用。 */
function choices(): ModLoaderChoice[] {
  return [
    {
      kind: "fabric",
      label: "Fabric",
      available: true,
      reason: null,
      versions: ["0.16.9", "0.15.11"],
      companions: [
        { label: "Fabric API", versions: ["0.116.6+1.21.1"], reason: null },
      ],
    },
    {
      kind: "quilt",
      label: "Quilt",
      available: false,
      reason: "与 Fabric 不兼容",
      versions: [],
      companions: [],
    },
    {
      kind: "forge",
      label: "Forge",
      available: false,
      reason: "这个版本只能用 NeoForge",
      versions: [],
      companions: [],
    },
    {
      kind: "neoforge",
      label: "NeoForge",
      available: true,
      reason: null,
      versions: ["21.1.72", "21.1.68", "21.1.60"],
      companions: [],
    },
  ];
}

function renderPicker(onInstalled?: (instance: InstanceSummary) => void) {
  return renderWithState(
    <ModLoaderPicker gameVersion="1.21.1" onInstalled={onInstalled} />,
  );
}

/** 某个加载器那一行本身；行名就是加载器的 label（等列表读完再找）。 */
function rowOf(label: string): Promise<HTMLElement> {
  return screen.findByRole("button", { name: label });
}

async function openLoader(label: string) {
  fireEvent.click(await rowOf(label));
}

beforeEach(() => {
  listVersions.mockReset();
  installLoader.mockReset();
  listVersions.mockImplementation(async () => choices());
  pretendDesktop();
});

afterEach(() => {
  pretendBrowser();
});

describe("ModLoaderPicker", () => {
  it("lists the four loaders with Forge first and says which can be added", async () => {
    renderPicker();

    const list = await screen.findByRole("list", { name: "模组加载器" });
    const rows = within(list).getAllByRole("button");
    expect(rows).toHaveLength(4);
    // forge / neoforge / fabric / quilt：固定顺序，不随后端返回顺序变
    expect(list).toHaveTextContent(/^Forge/);
    expect(within(list).getByText("NeoForge")).toBeInTheDocument();
    expect(within(list).getByText("Fabric")).toBeInTheDocument();
    expect(within(list).getByText("Quilt")).toBeInTheDocument();

    // 两个可用、两个写明原因
    // 还没选版本时，可用的行写「未选择版本」；选了版本这里就变成版本号。
    expect(within(list).getAllByText("未选择版本")).toHaveLength(2);
    expect(await rowOf("Forge")).toHaveTextContent("这个版本只能用 NeoForge");
    // 不兼容的原因由后端给，前端原样显示。
    expect(await rowOf("Quilt")).toHaveTextContent("与 Fabric 不兼容");
    expect(listVersions).toHaveBeenCalledWith("1.21.1");
  });

  it("marks a loader the version cannot use as disabled and shows the reason", async () => {
    renderPicker();
    await screen.findByRole("list", { name: "模组加载器" });

    const quilt = await rowOf("Quilt");
    expect(quilt).toHaveAttribute("aria-disabled", "true");
    expect(quilt).toHaveAttribute("data-available", "false");
    // 不可用的行没有展开状态，点了也不展开
    expect(quilt).not.toHaveAttribute("aria-expanded");
    expect(quilt).toHaveTextContent("与 Fabric 不兼容");

    fireEvent.click(quilt);

    expect(screen.queryByRole("listbox", { name: "Quilt 版本" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "安装" })).not.toBeInTheDocument();
  });

  it("expands one loader's versions when its row is clicked, and collapses it again", async () => {
    renderPicker();

    await openLoader("NeoForge");

    const versions = await screen.findByRole("listbox", { name: "NeoForge 版本" });
    expect(within(versions).getAllByRole("option")).toHaveLength(3);
    expect(within(versions).getByText("21.1.72")).toBeInTheDocument();
    // 同一时间只展开一行
    expect(screen.queryByRole("listbox", { name: "Fabric 版本" })).not.toBeInTheDocument();
    expect(await rowOf("NeoForge")).toHaveAttribute("aria-expanded", "true");

    fireEvent.click(await rowOf("NeoForge"));

    expect(screen.queryByRole("listbox", { name: "NeoForge 版本" })).not.toBeInTheDocument();
  });

  it("offers an install button once a version is picked", async () => {
    renderPicker();

    await openLoader("Fabric");
    const versions = await screen.findByRole("listbox", { name: "Fabric 版本" });
    // 还没选版本：有按钮但不可点
    expect(screen.getByRole("button", { name: "安装" })).toBeDisabled();

    fireEvent.click(within(versions).getByRole("option", { name: /0\.15\.11/ }));

    await waitFor(() => expect(screen.getByRole("button", { name: "安装" })).toBeEnabled());
    expect(within(versions).getByRole("option", { name: /0\.15\.11/ })).toHaveAttribute("aria-selected", "true");
    expect(within(versions).getByRole("option", { name: /0\.16\.9/ })).toHaveAttribute("aria-selected", "false");
    expect(
      screen.getByText("将安装 Fabric 0.15.11：先装游戏本体，再装上加载器并新建一个实例。"),
    ).toBeInTheDocument();
  });

  it("shows the picked version on the row and lists Fabric API", async () => {
    renderPicker();

    await openLoader("Fabric");
    const versions = await screen.findByRole("listbox", { name: "Fabric 版本" });
    fireEvent.click(within(versions).getByRole("option", { name: /0\.15\.11/ }));

    // 选中的版本要显示在行上（收起列表后也还在）。
    // 选中的版本要显示在行上（列表里也有一份，所以查行本身）。
    await waitFor(() => expect(screen.getByRole("button", { name: "Fabric" })).toHaveTextContent("0.15.11"));

    // Fabric API 是随 Fabric 一起装的附属组件。
    expect(screen.getByText("Fabric API")).toBeInTheDocument();
    expect(screen.getByText("0.116.6+1.21.1")).toBeInTheDocument();
    expect(screen.getByText("会一起装上")).toBeInTheDocument();

    // 只有「最新」那个版本带标签，其它行不带。
    expect(within(versions).getByText("最新")).toBeInTheDocument();
    expect(within(versions).queryByText("可选")).not.toBeInTheDocument();
  });

  it("installs the chosen loader and reports the new instance", async () => {
    let finish: (result: ModLoaderInstallResult) => void = () => undefined;
    installLoader.mockImplementation(
      () =>
        new Promise<ModLoaderInstallResult>((resolve) => {
          finish = resolve;
        }),
    );
    const onInstalled = vi.fn();
    renderPicker(onInstalled);

    await openLoader("NeoForge");
    fireEvent.click(await screen.findByRole("option", { name: /21\.1\.72/ }));
    fireEvent.click(screen.getByRole("button", { name: "安装" }));

    await waitFor(() => expect(installLoader).toHaveBeenCalledWith("neoforge", "1.21.1", "21.1.72"));
    // 安装中：按钮禁用并换成进度文字
    expect(screen.getByRole("button", { name: "正在安装…" })).toBeDisabled();

    finish({
      instance: { id: "abc", name: "1.21.1-neoforge-21.1.72", version: "1.21.1-neoforge-21.1.72", state: "ready" },
      versionId: "1.21.1-neoforge-21.1.72",
    });

    expect(await screen.findByText(/已装好，新实例已经建好了/)).toBeInTheDocument();
    expect(onInstalled).toHaveBeenCalledWith(
      expect.objectContaining({ id: "abc", version: "1.21.1-neoforge-21.1.72" }),
    );
    expect(screen.getByRole("button", { name: "安装" })).toBeEnabled();
  });

  it("shows the reason the backend gave when the install fails", async () => {
    installLoader.mockRejectedValue("modloader installer is not available for 1.21.1");
    renderPicker();

    await openLoader("Fabric");
    fireEvent.click(await screen.findByRole("option", { name: /0\.16\.9/ }));
    fireEvent.click(screen.getByRole("button", { name: "安装" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "modloader installer is not available for 1.21.1",
    );
    // 失败后能再试一次
    expect(screen.getByRole("button", { name: "安装" })).toBeEnabled();
  });

  it("offers a retry when the list could not be read at all", async () => {
    listVersions.mockImplementation(async () => []);
    renderPicker();

    expect(await screen.findByRole("alert")).toHaveTextContent("没能读取模组加载器列表");

    listVersions.mockImplementation(async () => choices());
    fireEvent.click(screen.getByRole("button", { name: "重新读取" }));

    expect(await screen.findByRole("list", { name: "模组加载器" })).toBeInTheDocument();
  });

  it("says it needs the desktop build when running in a browser", () => {
    pretendBrowser();
    renderPicker();

    expect(screen.getByText(/需要桌面版/)).toBeInTheDocument();
    expect(listVersions).not.toHaveBeenCalled();
  });
});
