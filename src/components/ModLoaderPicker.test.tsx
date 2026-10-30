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

/**
 * 四个加载器，故意不按固定顺序返回。
 *
 * 全部可装：这样才测得出「不兼容」是按**已选项**算出来的，而不是一开始就写死。
 */
function choices(): ModLoaderChoice[] {
  return [
    {
      kind: "fabric",
      label: "Fabric",
      available: true,
      reason: null,
      versions: ["0.16.9", "0.15.11"],
      companions: [{ label: "Fabric API", versions: ["0.116.6+1.21.1"], reason: null }],
    },
    {
      kind: "quilt",
      label: "Quilt",
      available: true,
      reason: null,
      versions: ["0.27.0"],
      companions: [],
    },
    {
      kind: "forge",
      label: "Forge",
      available: true,
      reason: null,
      versions: ["66.1.2", "66.0.0"],
      companions: [],
    },
    {
      kind: "neoforge",
      label: "NeoForge",
      available: true,
      reason: null,
      versions: ["21.1.72", "21.1.68"],
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

/** 展开某个加载器并选中一个版本 —— 选中会让列表收回去。 */
async function chooseVersion(label: string, version: string) {
  await openLoader(label);
  const list = await screen.findByRole("listbox", { name: `${label} 版本` });
  fireEvent.click(within(list).getByRole("option", { name: new RegExp(version.replace(/\./g, "\\.")) }));
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
  it("lists the four loaders in a fixed order and offers them all as addable", async () => {
    renderPicker();
    const list = await screen.findByRole("list", { name: "模组加载器" });

    const names = within(list)
      .getAllByRole("button", { name: /^(Forge|NeoForge|Fabric|Quilt)$/ })
      .map((item) => item.getAttribute("aria-label"));
    expect(names).toEqual(["Forge", "NeoForge", "Fabric", "Quilt"]);

    // 还没选任何加载器：四行都只是「可以添加」，没有不兼容的说法。
    expect(within(list).getAllByText("可以添加")).toHaveLength(4);
    expect(within(list).queryByText(/不兼容/)).not.toBeInTheDocument();
    expect(listVersions).toHaveBeenCalledWith("1.21.1");
  });

  it("collapses the version list as soon as a version is picked", async () => {
    renderPicker();

    await openLoader("Fabric");
    const versions = await screen.findByRole("listbox", { name: "Fabric 版本" });
    expect(within(versions).getAllByRole("option")).toHaveLength(2);

    fireEvent.click(within(versions).getByRole("option", { name: /0\.15\.11/ }));

    // 选完就收回去 —— PCL 的 CardXxx.IsSwapped = True。
    expect(screen.queryByRole("listbox", { name: "Fabric 版本" })).not.toBeInTheDocument();
    // 行上显示选中的版本。
    expect(await rowOf("Fabric")).toHaveTextContent("0.15.11");
  });

  it("shows the incompatibility only against what is already selected", async () => {
    renderPicker();
    await screen.findByRole("list", { name: "模组加载器" });

    await chooseVersion("Fabric", "0.16.9");

    // 选了 Fabric 之后，Forge 与 NeoForge 才被标成不兼容。
    expect(await rowOf("Forge")).toHaveTextContent("与 Fabric 不兼容");
    expect(await rowOf("NeoForge")).toHaveTextContent("与 Fabric 不兼容");
    // Quilt 与 Fabric 同族，可以共存，不标。
    expect(await rowOf("Quilt")).not.toHaveTextContent("不兼容");
    // Fabric 自己当然也不标。
    expect(await rowOf("Fabric")).not.toHaveTextContent("不兼容");
  });

  it("does not let a conflicting loader be expanded", async () => {
    renderPicker();
    await screen.findByRole("list", { name: "模组加载器" });

    await chooseVersion("Forge", "66.1.2");

    const fabric = await rowOf("Fabric");
    expect(fabric).toHaveTextContent("与 Forge 不兼容");
    expect(fabric).toHaveAttribute("aria-disabled", "true");

    fireEvent.click(fabric);

    // 点不开。
    expect(screen.queryByRole("listbox", { name: "Fabric 版本" })).not.toBeInTheDocument();
  });

  it("clears a pick with X and brings back the other loaders", async () => {
    renderPicker();
    await screen.findByRole("list", { name: "模组加载器" });

    await chooseVersion("Fabric", "0.16.9");
    expect(await rowOf("Forge")).toHaveTextContent("与 Fabric 不兼容");

    fireEvent.click(screen.getByRole("button", { name: "取消选择 Fabric" }));

    // 清掉之后回到「可以添加」，其他行也不再被标不兼容。
    await waitFor(async () => expect(await rowOf("Fabric")).toHaveTextContent("可以添加"));
    expect(await rowOf("Forge")).not.toHaveTextContent("不兼容");
    expect(screen.queryByRole("button", { name: "取消选择 Fabric" })).not.toBeInTheDocument();
  });

  it("reveals Fabric API once Fabric is picked and preselects it", async () => {
    renderPicker();
    await screen.findByRole("list", { name: "模组加载器" });

    // 没选 Fabric 之前没有这张卡。
    expect(screen.queryByText("Fabric API")).not.toBeInTheDocument();

    await chooseVersion("Fabric", "0.16.9");

    expect(await screen.findByText("Fabric API")).toBeInTheDocument();
    expect(screen.getByText("0.116.6+1.21.1")).toBeInTheDocument();

    // 可以单独取消。
    fireEvent.click(screen.getByRole("button", { name: "取消选择 Fabric API" }));
    expect(screen.queryByText("0.116.6+1.21.1")).not.toBeInTheDocument();
  });

  it("hides Fabric API again when Fabric is cleared", async () => {
    renderPicker();
    await screen.findByRole("list", { name: "模组加载器" });

    await chooseVersion("Fabric", "0.16.9");
    expect(await screen.findByText("Fabric API")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "取消选择 Fabric" }));

    await waitFor(() => expect(screen.queryByText("Fabric API")).not.toBeInTheDocument());
  });

  it("tags only the newest version", async () => {
    renderPicker();

    await openLoader("Forge");
    const versions = await screen.findByRole("listbox", { name: "Forge 版本" });

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

    await chooseVersion("NeoForge", "21.1.72");

    // 「开始安装」在列表外面，列表收起后仍然可用。
    expect(screen.queryByRole("listbox", { name: "NeoForge 版本" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "开始安装" }));

    await waitFor(() => expect(installLoader).toHaveBeenCalledWith("neoforge", "1.21.1", "21.1.72"));
    expect(screen.getByRole("button", { name: "正在安装…" })).toBeDisabled();

    finish({
      instance: {
        id: "abc",
        name: "1.21.1-neoforge-21.1.72",
        version: "1.21.1-neoforge-21.1.72",
        state: "ready",
      },
      versionId: "1.21.1-neoforge-21.1.72",
    });

    expect(await screen.findByText(/已装好，新实例已经建好了/)).toBeInTheDocument();
    expect(onInstalled).toHaveBeenCalledWith(
      expect.objectContaining({ id: "abc", version: "1.21.1-neoforge-21.1.72" }),
    );
  });

  it("shows the reason the backend gave when the install fails", async () => {
    installLoader.mockRejectedValue("modloader installer is not available for 1.21.1");
    renderPicker();

    await chooseVersion("Fabric", "0.16.9");
    fireEvent.click(screen.getByRole("button", { name: "开始安装" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "modloader installer is not available for 1.21.1",
    );
  });

  it("says a loader cannot be used when the backend reports it unavailable", async () => {
    listVersions.mockImplementation(async () =>
      choices().map((choice) =>
        choice.kind === "quilt"
          ? { ...choice, available: false, reason: "Quilt 没有支持 1.21.1 的版本", versions: [] }
          : choice,
      ),
    );
    renderPicker();
    await screen.findByRole("list", { name: "模组加载器" });

    const quilt = await rowOf("Quilt");
    expect(quilt).toHaveAttribute("aria-disabled", "true");
    expect(quilt).toHaveTextContent("Quilt 没有支持 1.21.1 的版本");

    fireEvent.click(quilt);
    expect(screen.queryByRole("listbox", { name: "Quilt 版本" })).not.toBeInTheDocument();
  });

  it("offers a retry when the list could not be read at all", async () => {
    listVersions.mockImplementation(async () => []);
    renderPicker();

    expect(await screen.findByRole("alert")).toHaveTextContent("没能读取模组加载器列表");

    listVersions.mockImplementation(async () => choices());
    fireEvent.click(screen.getByRole("button", { name: "重新读取" }));

    expect(await rowOf("Fabric")).toBeInTheDocument();
  });

  it("says it needs the desktop build when running in a browser", async () => {
    pretendBrowser();
    renderPicker();

    expect(screen.getByText(/需要桌面版/)).toBeInTheDocument();
    expect(listVersions).not.toHaveBeenCalled();
  });
});
