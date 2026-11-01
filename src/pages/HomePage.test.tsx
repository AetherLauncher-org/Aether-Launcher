import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { InstanceSummary } from "../ipc";
import { useInstall } from "../state/install";
import { pretendBrowser, pretendDesktop, renderWithState } from "../test/support";
import { HomePage } from "./HomePage";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));

vi.mock("../ipc", async (importOriginal) => {
  const original = await importOriginal<typeof import("../ipc")>();
  return {
    ...original,
    cancelVanillaInstall: vi.fn(async () => true),
    discoverJavaRuntimes: vi.fn(async () => []),
    installModLoaderVersion: vi.fn(),
    listInstances: vi.fn(async () => ({ instances: [], skipped: [] })),
    listenInstallProgress: vi.fn(async () => () => undefined),
    startVanillaInstall: vi.fn(),
  };
});

const { installModLoaderVersion, startVanillaInstall } = await import("../ipc");
const installLoader = vi.mocked(installModLoaderVersion);
const installVanilla = vi.mocked(startVanillaInstall);

const fabric = { kind: "fabric", version: "0.16.9" } as const;

const instance: InstanceSummary = {
  id: "abc",
  name: "1.21.1",
  version: "1.21.1",
  state: "ready",
};

const loaderResult = {
  instance: { ...instance, name: "1.21.1-Fabric 0.16.9", version: "fabric-loader-0.16.9-1.21.1" },
  versionId: "fabric-loader-0.16.9-1.21.1",
};

/**
 * 安装是在下载页发起的，但任务挂在应用层的 InstallProvider 上：装到一半切回主页，
 * 主页会显示同一份进度和失败卡片。这里就在同一棵 Provider 树里把那次安装发起来，
 * 让主页上的「重试」按钮走的是真实链路，而不是直接调 Provider 的方法。
 */
function StartInstall() {
  const { start } = useInstall();
  return (
    <>
      <button
        type="button"
        onClick={() => void start("1.21.1", "1.21.1-Fabric 0.16.9", fabric)}
      >
        装 Fabric
      </button>
      <button type="button" onClick={() => void start("1.21.1", "1.21.1")}>
        装原版
      </button>
    </>
  );
}

function renderHome() {
  return renderWithState(
    <>
      <StartInstall />
      <HomePage greeting="下午好" summary="继续上次的游戏" />
    </>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  // 一次性实现（…Once）不随 clearAllMocks 清掉，会串到下一个用例；这两个每次显式重置。
  installLoader.mockReset();
  installVanilla.mockReset();
  pretendDesktop();
});

afterEach(() => {
  pretendBrowser();
});

describe("HomePage install failure", () => {
  it("retries a failed loader install as the same loader", async () => {
    installLoader.mockRejectedValueOnce("network error").mockResolvedValueOnce(loaderResult as never);
    renderHome();

    fireEvent.click(screen.getByRole("button", { name: "装 Fabric" }));

    const failure = await screen.findByRole("alert");
    expect(failure).toHaveTextContent("安装 1.21.1 失败：network error");
    // 失败卡片上只有一个重试入口。
    expect(within(failure).getAllByRole("button", { name: "重试" })).toHaveLength(1);

    fireEvent.click(within(failure).getByRole("button", { name: "重试" }));

    // 主页的重试必须沿用「带加载器」的那次安装；丢了加载器就会悄悄变成装原版。
    // 重试是同步发起的（retry() 返回 void），点完就能看到实际走了哪条命令。
    expect(installVanilla, "重试丢了加载器：这次点击变成了原版安装").not.toHaveBeenCalled();
    expect(installLoader, "重试应当再走一次同一个加载器").toHaveBeenCalledTimes(2);
    expect(installLoader).toHaveBeenLastCalledWith(
      "fabric",
      "1.21.1",
      "0.16.9",
      "1.21.1-Fabric 0.16.9",
    );

    // 重试成功后失败卡片自动消失。
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
  });

  it("retries a failed vanilla install as vanilla", async () => {
    installVanilla.mockRejectedValueOnce("network error").mockResolvedValueOnce(instance as never);
    renderHome();

    fireEvent.click(screen.getByRole("button", { name: "装原版" }));

    const failure = await screen.findByRole("alert");
    fireEvent.click(within(failure).getByRole("button", { name: "重试" }));

    await waitFor(() => expect(installVanilla).toHaveBeenCalledTimes(2));
    expect(installVanilla).toHaveBeenLastCalledWith("1.21.1", "1.21.1");
    expect(installLoader).not.toHaveBeenCalled();
  });

  it("offers no retry while the install is still running", async () => {
    let finish: () => void = () => undefined;
    installLoader.mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = () => resolve(loaderResult as never);
        }) as never,
    );
    renderHome();

    fireEvent.click(screen.getByRole("button", { name: "装 Fabric" }));

    expect(await screen.findByText("正在安装 1.21.1")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "重试" })).not.toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "取消" })).toHaveLength(1);

    await act(async () => {
      finish();
    });
    await waitFor(() => expect(screen.queryByRole("button", { name: "取消" })).not.toBeInTheDocument());
  });
});
