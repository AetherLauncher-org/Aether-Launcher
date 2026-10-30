import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MinecraftCatalogVersion, ModLoaderChoice } from "../ipc";
import { pretendBrowser, pretendDesktop, renderWithState } from "../test/support";
import { VersionDetail } from "./VersionDetail";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));

vi.mock("../ipc", async (importOriginal) => {
  const original = await importOriginal<typeof import("../ipc")>();
  return {
    ...original,
    listModLoaderVersions: vi.fn(),
    startVanillaInstall: vi.fn(),
    installModLoaderVersion: vi.fn(),
    listInstances: vi.fn(async () => ({ instances: [], skipped: [] })),
  };
});

const { listModLoaderVersions, startVanillaInstall, installModLoaderVersion, listInstances } =
  await import("../ipc");
const listVersions = vi.mocked(listModLoaderVersions);
const startVanilla = vi.mocked(startVanillaInstall);
const installLoader = vi.mocked(installModLoaderVersion);
const mockListInstances = vi.mocked(listInstances);

const version: MinecraftCatalogVersion = {
  id: "1.21.1",
  type: "release",
  releaseTime: "2024-08-08T13:34:05+00:00",
  sha1: "0123456789abcdef0123456789abcdef01234567",
  url: "https://piston-meta.mojang.com/v1/packages/x/1.21.1.json",
};

function choices(): ModLoaderChoice[] {
  return [
    {
      kind: "fabric",
      label: "Fabric",
      available: true,
      reason: null,
      versions: ["0.16.9"],
      companions: [{ label: "Fabric API", versions: ["0.116.6+1.21.1"], reason: null }],
    },
    { kind: "forge", label: "Forge", available: true, reason: null, versions: ["66.1.2"], companions: [] },
    { kind: "neoforge", label: "NeoForge", available: false, reason: "无", versions: [], companions: [] },
    { kind: "quilt", label: "Quilt", available: false, reason: "无", versions: [], companions: [] },
  ];
}

function renderDetail() {
  return renderWithState(<VersionDetail version={version} isLatest onBack={() => undefined} />);
}

function nameInput(): HTMLInputElement {
  return screen.getByLabelText("实例名称") as HTMLInputElement;
}

beforeEach(() => {
  vi.clearAllMocks();
  listVersions.mockImplementation(async () => choices());
  mockListInstances.mockResolvedValue({ instances: [], skipped: [] } as never);
  pretendDesktop();
});

afterEach(() => {
  pretendBrowser();
});

describe("VersionDetail", () => {
  it("has exactly one install button", async () => {
    renderDetail();
    await screen.findByRole("list", { name: "模组加载器" });

    // 加载器卡片不能自带安装按钮，页面上只允许这一个。
    const buttons = screen.getAllByRole("button", { name: /^安装/ });
    expect(buttons).toHaveLength(1);
    expect(buttons[0]).toHaveTextContent("安装");
  });

  it("renames the instance when a loader is picked and restores it when cleared", async () => {
    renderDetail();
    await screen.findByRole("list", { name: "模组加载器" });

    expect(nameInput().value).toBe("1.21.1");

    // 选 Fabric 0.16.9
    fireEvent.click(await screen.findByRole("button", { name: "Fabric" }));
    const list = await screen.findByRole("listbox", { name: "Fabric 版本" });
    fireEvent.click(within(list).getByRole("option", { name: /0\.16\.9/ }));

    await waitFor(() => expect(nameInput().value).toBe("1.21.1-Fabric 0.16.9"));
    // 按钮文案也说明装什么。
    expect(screen.getByRole("button", { name: /^安装/ })).toHaveTextContent("Fabric 0.16.9");

    // X 取消 → 名字回到原版
    fireEvent.click(screen.getByRole("button", { name: "取消选择 Fabric" }));
    await waitFor(() => expect(nameInput().value).toBe("1.21.1"));
  });

  it("stops following the loader once the name is typed", async () => {
    renderDetail();
    await screen.findByRole("list", { name: "模组加载器" });

    fireEvent.change(nameInput(), { target: { value: "我的整合包" } });

    fireEvent.click(await screen.findByRole("button", { name: "Fabric" }));
    const list = await screen.findByRole("listbox", { name: "Fabric 版本" });
    fireEvent.click(within(list).getByRole("option", { name: /0\.16\.9/ }));

    // 用户改过名字，就不再被覆盖。
    expect(nameInput().value).toBe("我的整合包");
  });

  it("installs the loader through the single button", async () => {
    installLoader.mockResolvedValue({
      instance: { id: "abc", name: "1.21.1-Fabric 0.16.9", version: "fabric-loader-0.16.9-1.21.1", state: "ready" },
      versionId: "fabric-loader-0.16.9-1.21.1",
    } as never);
    renderDetail();
    await screen.findByRole("list", { name: "模组加载器" });

    fireEvent.click(await screen.findByRole("button", { name: "Fabric" }));
    const list = await screen.findByRole("listbox", { name: "Fabric 版本" });
    fireEvent.click(within(list).getByRole("option", { name: /0\.16\.9/ }));
    fireEvent.click(screen.getByRole("button", { name: /^安装/ }));

    await waitFor(() =>
      expect(installLoader).toHaveBeenCalledWith("fabric", "1.21.1", "0.16.9", "1.21.1-Fabric 0.16.9"),
    );
    // 选了加载器就不该再单独走原版安装。
    expect(startVanilla).not.toHaveBeenCalled();
  });

  it("installs plain Vanilla when no loader is picked", async () => {
    startVanilla.mockResolvedValue({
      id: "v1",
      name: "1.21.1",
      version: "1.21.1",
      state: "ready",
    } as never);
    renderDetail();
    await screen.findByRole("list", { name: "模组加载器" });

    fireEvent.click(screen.getByRole("button", { name: /^安装/ }));

    await waitFor(() => expect(startVanilla).toHaveBeenCalledWith("1.21.1", "1.21.1"));
    expect(installLoader).not.toHaveBeenCalled();
  });
});
