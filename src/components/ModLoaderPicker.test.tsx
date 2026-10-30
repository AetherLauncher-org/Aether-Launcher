import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ModLoaderChoice } from "../ipc";
import { pretendBrowser, pretendDesktop, renderWithState } from "../test/support";
import { ModLoaderPicker } from "./ModLoaderPicker";
import type { ModLoaderSelection } from "./ModLoaderPicker";

// 直接 mock IPC 封装：这个组件只关心「读到了什么」。
vi.mock("../ipc", async (importOriginal) => {
  const original = await importOriginal<typeof import("../ipc")>();
  return { ...original, listModLoaderVersions: vi.fn() };
});

const { listModLoaderVersions } = await import("../ipc");
const listVersions = vi.mocked(listModLoaderVersions);

/** 四个加载器，故意不按固定顺序返回，全部可装。 */
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

/** 受控组件：用一层 state 包起来，并记录每次变化给测试断言。 */
function renderPicker(initial: ModLoaderSelection | null = null) {
  const changes: (ModLoaderSelection | null)[] = [];

  function Host() {
    const [value, setValue] = useState<ModLoaderSelection | null>(initial);
    return (
      <ModLoaderPicker
        gameVersion="1.21.1"
        value={value}
        onChange={(next) => {
          changes.push(next);
          setValue(next);
        }}
      />
    );
  }

  renderWithState(<Host />);
  return { changes };
}

function rowOf(label: string): Promise<HTMLElement> {
  return screen.findByRole("button", { name: label });
}

async function openLoader(label: string) {
  fireEvent.click(await rowOf(label));
}

async function chooseVersion(label: string, version: string) {
  await openLoader(label);
  const list = await screen.findByRole("listbox", { name: `${label} 版本` });
  fireEvent.click(
    within(list).getByRole("option", { name: new RegExp(version.replace(/\./g, "\\.")) }),
  );
}

beforeEach(() => {
  listVersions.mockReset();
  listVersions.mockImplementation(async () => choices());
  pretendDesktop();
});

afterEach(() => {
  pretendBrowser();
});

describe("ModLoaderPicker", () => {
  it("lists the four loaders in a fixed order with nothing preselected", async () => {
    renderPicker();
    const list = await screen.findByRole("list", { name: "模组加载器" });

    const names = within(list)
      .getAllByRole("button", { name: /^(Forge|NeoForge|Fabric|Quilt)$/ })
      .map((item) => item.getAttribute("aria-label"));
    expect(names).toEqual(["Forge", "NeoForge", "Fabric", "Quilt"]);

    expect(within(list).getAllByText("可以添加")).toHaveLength(4);
    expect(within(list).queryByText(/不兼容/)).not.toBeInTheDocument();
    expect(listVersions).toHaveBeenCalledWith("1.21.1");
  });

  it("collapses the list and reports the pick", async () => {
    const { changes } = renderPicker();

    await chooseVersion("Fabric", "0.15.11");

    expect(screen.queryByRole("listbox", { name: "Fabric 版本" })).not.toBeInTheDocument();
    expect(await rowOf("Fabric")).toHaveTextContent("0.15.11");
    expect(changes[changes.length - 1]).toEqual({
      kind: "fabric",
      label: "Fabric",
      version: "0.15.11",
      companion: expect.objectContaining({ label: "Fabric API" }),
    });
  });

  it("has no install button of its own", async () => {
    renderPicker();
    await screen.findByRole("list", { name: "模组加载器" });

    // 安装按钮归 VersionDetail，这里不能有第二个。
    expect(screen.queryByRole("button", { name: /安装/ })).not.toBeInTheDocument();
  });

  it("shows the incompatibility only against what is already selected", async () => {
    renderPicker();
    await screen.findByRole("list", { name: "模组加载器" });

    await chooseVersion("Fabric", "0.16.9");

    expect(await rowOf("Forge")).toHaveTextContent("与 Fabric 不兼容");
    expect(await rowOf("NeoForge")).toHaveTextContent("与 Fabric 不兼容");
    // Quilt 与 Fabric 同族，可以共存。
    expect(await rowOf("Quilt")).not.toHaveTextContent("不兼容");
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
    expect(screen.queryByRole("listbox", { name: "Fabric 版本" })).not.toBeInTheDocument();
  });

  it("clears a pick with X and unblocks the others", async () => {
    const { changes } = renderPicker();
    await screen.findByRole("list", { name: "模组加载器" });

    await chooseVersion("Fabric", "0.16.9");
    expect(await rowOf("Forge")).toHaveTextContent("与 Fabric 不兼容");

    fireEvent.click(screen.getByRole("button", { name: "取消选择 Fabric" }));

    await waitFor(async () => expect(await rowOf("Fabric")).toHaveTextContent("可以添加"));
    expect(await rowOf("Forge")).not.toHaveTextContent("不兼容");
    expect(changes[changes.length - 1]).toBeNull();
  });

  it("reveals Fabric API once Fabric is picked and preselects it", async () => {
    const { changes } = renderPicker();
    await screen.findByRole("list", { name: "模组加载器" });

    expect(screen.queryByText("Fabric API")).not.toBeInTheDocument();

    await chooseVersion("Fabric", "0.16.9");

    expect(await screen.findByText("Fabric API")).toBeInTheDocument();
    expect(screen.getByText("0.116.6+1.21.1")).toBeInTheDocument();
    expect(changes[changes.length - 1]?.companion).toEqual({ label: "Fabric API", version: "0.116.6+1.21.1" });

    // 可以单独取消，Fabric 本身保留。
    fireEvent.click(screen.getByRole("button", { name: "取消选择 Fabric API" }));
    expect(changes[changes.length - 1]?.kind).toBe("fabric");
    expect(changes[changes.length - 1]?.companion).toBeNull();
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
