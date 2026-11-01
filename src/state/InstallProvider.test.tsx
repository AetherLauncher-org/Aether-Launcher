import { fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { InstanceSummary } from "../ipc";
import { pretendBrowser, pretendDesktop, renderWithState } from "../test/support";
import { useInstall } from "./install";

vi.mock("../ipc", async (importOriginal) => {
  const original = await importOriginal<typeof import("../ipc")>();
  return {
    ...original,
    cancelVanillaInstall: vi.fn(async () => true),
    installModLoaderVersion: vi.fn(),
    listenInstallProgress: vi.fn(async () => () => undefined),
    listInstances: vi.fn(async () => ({ instances: [], skipped: [] })),
    startVanillaInstall: vi.fn(),
  };
});

const { installModLoaderVersion, listInstances, startVanillaInstall } = await import("../ipc");
const installLoader = vi.mocked(installModLoaderVersion);
const installVanilla = vi.mocked(startVanillaInstall);
const mockListInstances = vi.mocked(listInstances);

const instance: InstanceSummary = {
  id: "abc",
  name: "1.21.1",
  version: "1.21.1",
  state: "ready",
};

/** 把 Context 里的东西摆成一屏按钮，方便直接驱动安装与重试。 */
function Probe() {
  const { job, start, retry } = useInstall();
  return (
    <div>
      <span data-testid="status">{job === null ? "none" : job.status}</span>
      <span data-testid="loader">
        {job?.loader ? job.loader.kind + " " + job.loader.version : "none"}
      </span>
      <button
        type="button"
        onClick={() =>
          void start("1.21.1", "1.21.1-Fabric 0.16.9", { kind: "fabric", version: "0.16.9" })
        }
      >
        装加载器
      </button>
      <button type="button" onClick={() => void start("1.21.1", "1.21.1")}>
        装原版
      </button>
      <button type="button" onClick={retry}>
        重试
      </button>
    </div>
  );
}

function renderProbe() {
  return renderWithState(<Probe />);
}

function status(): string {
  return screen.getByTestId("status").textContent ?? "";
}

function loader(): string {
  return screen.getByTestId("loader").textContent ?? "";
}

beforeEach(() => {
  vi.clearAllMocks();
  mockListInstances.mockResolvedValue({ instances: [], skipped: [] } as never);
  pretendDesktop();
});

afterEach(() => {
  pretendBrowser();
});

describe("InstallProvider", () => {
  it("records the mod loader on the job", async () => {
    installLoader.mockResolvedValue({
      instance: { ...instance, name: "1.21.1-Fabric 0.16.9" },
      versionId: "fabric-loader-0.16.9-1.21.1",
    } as never);
    renderProbe();

    fireEvent.click(screen.getByRole("button", { name: "装加载器" }));

    await waitFor(() => expect(status()).toBe("done"));
    expect(installLoader).toHaveBeenCalledWith("fabric", "1.21.1", "0.16.9", "1.21.1-Fabric 0.16.9");
    expect(installVanilla).not.toHaveBeenCalled();
    expect(loader()).toBe("fabric 0.16.9");
  });

  it("leaves the loader empty for a vanilla install", async () => {
    installVanilla.mockResolvedValue(instance as never);
    renderProbe();

    fireEvent.click(screen.getByRole("button", { name: "装原版" }));

    await waitFor(() => expect(status()).toBe("done"));
    expect(installVanilla).toHaveBeenCalledWith("1.21.1", "1.21.1");
    expect(loader()).toBe("none");
  });

  it("retries a failed loader install as the same loader", async () => {
    installLoader
      .mockRejectedValueOnce("network error")
      .mockResolvedValueOnce({ instance, versionId: "fabric-loader-0.16.9-1.21.1" } as never);
    renderProbe();

    fireEvent.click(screen.getByRole("button", { name: "装加载器" }));
    await waitFor(() => expect(status()).toBe("error"));

    fireEvent.click(screen.getByRole("button", { name: "重试" }));

    await waitFor(() => expect(installLoader).toHaveBeenCalledTimes(2));
    expect(installLoader).toHaveBeenLastCalledWith(
      "fabric",
      "1.21.1",
      "0.16.9",
      "1.21.1-Fabric 0.16.9",
    );
    // 重试丢掉加载器的话，这里会变成一个只装了原版的实例。
    expect(installVanilla).not.toHaveBeenCalled();
    expect(loader()).toBe("fabric 0.16.9");
  });

  it("retries a failed vanilla install as vanilla", async () => {
    installVanilla
      .mockRejectedValueOnce("network error")
      .mockResolvedValueOnce(instance as never);
    renderProbe();

    fireEvent.click(screen.getByRole("button", { name: "装原版" }));
    await waitFor(() => expect(status()).toBe("error"));

    fireEvent.click(screen.getByRole("button", { name: "重试" }));

    await waitFor(() => expect(installVanilla).toHaveBeenCalledTimes(2));
    expect(installVanilla).toHaveBeenLastCalledWith("1.21.1", "1.21.1");
    expect(installLoader).not.toHaveBeenCalled();
  });

  it("does nothing when retrying before any install", async () => {
    renderProbe();

    fireEvent.click(screen.getByRole("button", { name: "重试" }));

    expect(status()).toBe("none");
    expect(installVanilla).not.toHaveBeenCalled();
    expect(installLoader).not.toHaveBeenCalled();
  });

  it("ignores a retry while an install is in flight", async () => {
    let finish: (value: InstanceSummary) => void = () => undefined;
    installVanilla.mockImplementation(
      () => new Promise<InstanceSummary>((resolve) => { finish = resolve; }) as never,
    );
    renderProbe();

    fireEvent.click(screen.getByRole("button", { name: "装原版" }));
    fireEvent.click(screen.getByRole("button", { name: "重试" }));
    expect(installVanilla).toHaveBeenCalledTimes(1);

    finish(instance);
    await waitFor(() => expect(status()).toBe("done"));
  });

  it("keeps the running attempt when another start comes in", async () => {
    let finish: (value: unknown) => void = () => undefined;
    installLoader.mockImplementation(
      () => new Promise<unknown>((resolve) => { finish = resolve; }) as never,
    );
    renderProbe();

    fireEvent.click(screen.getByRole("button", { name: "装加载器" }));
    // 后端一次只允许一个安装：第二次点击既不能开始装原版，也不能把重试的目标改掉。
    fireEvent.click(screen.getByRole("button", { name: "装原版" }));
    expect(installVanilla).not.toHaveBeenCalled();

    finish({ instance, versionId: "fabric-loader-0.16.9-1.21.1" });
    await waitFor(() => expect(status()).toBe("done"));
    expect(loader()).toBe("fabric 0.16.9");
  });
});
