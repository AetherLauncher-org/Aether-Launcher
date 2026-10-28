import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { resetCatalogCache } from "../lib/catalogCache";
import type { PageDefinition } from "../navigation";
import { pretendBrowser, pretendDesktop, renderWithState, routeInvoke } from "../test/support";
import type { Handlers } from "../test/support";
import { DownloadPage } from "./DownloadPage";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const page: PageDefinition = {
  id: "download",
  label: "下载",
  summary: "从 Mojang 官方安装游戏版本",
  icon: <span />,
  activeIcon: <span />,
};

const sha = "0123456789abcdef0123456789abcdef01234567";
const entry = (id: string, type: string, releaseTime: string) => ({
  id,
  type,
  releaseTime,
  sha1: sha,
  url: `https://piston-meta.mojang.com/v1/packages/x/${id}.json`,
});

const catalog = {
  latest: { release: "1.21.1", snapshot: "24w14a" },
  versions: [
    entry("24w14a", "snapshot", "2024-04-03T12:00:00+00:00"),
    entry("1.21.1", "release", "2024-08-08T13:34:05+00:00"),
    entry("1.20.4", "release", "2023-12-07T12:56:20+00:00"),
    entry("1.12.2", "release", "2017-09-18T08:39:46+00:00"),
    entry("b1.7.3", "old_beta", "2011-07-08T00:00:00+00:00"),
  ],
};

const survival = { id: "i1", name: "Survival", version: "1.20.4", state: "ready" };
const created = { id: "i2", name: "1.21.1", version: "1.21.1", state: "ready" };

let progressHandler: (event: { payload: unknown }) => void = () => undefined;

function backend(overrides: Handlers = {}, instances: unknown[] = []) {
  routeInvoke(vi.mocked(invoke), {
    list_instances: () => ({ instances, skipped: [] }),
    list_minecraft_versions: () => catalog,
    ...overrides,
  });
}

/** 一个可以由测试决定何时结束的安装命令。 */
function pendingInstall() {
  let resolve: (value: unknown) => void = () => undefined;
  let reject: (reason: unknown) => void = () => undefined;
  const promise = new Promise((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject, handler: () => promise };
}

async function openVersion(id: string) {
  const row = (await screen.findAllByText(id)).map((node) => node.closest("button")).find(Boolean) as HTMLElement;
  fireEvent.click(row);
  await screen.findByRole("button", { name: "返回版本列表" });
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(listen).mockReset();
  vi.mocked(listen).mockImplementation(async (_name, callback) => {
    progressHandler = callback as typeof progressHandler;
    return () => undefined;
  });
  resetCatalogCache();
  pretendDesktop();
});

afterEach(() => {
  pretendBrowser();
});

describe("DownloadPage version browser", () => {
  it("shows the latest release and snapshot above the full release list", async () => {
    backend();
    renderWithState(<DownloadPage page={page} />);

    expect(await screen.findByText("最新正式版")).toBeInTheDocument();
    expect(screen.getByText("最新快照")).toBeInTheDocument();
    const list = screen.getByRole("list", { name: /正式版列表，共 3 个/ });
    expect(within(list).getByText("1.21.1")).toBeInTheDocument();
    expect(within(list).getByText("1.20.4")).toBeInTheDocument();
    expect(within(list).queryByText("24w14a")).not.toBeInTheDocument();
    expect(within(list).getByText("最新")).toBeInTheDocument();
  });

  it("groups versions by kind with tabs", async () => {
    backend();
    renderWithState(<DownloadPage page={page} />);
    await screen.findByText("最新正式版");

    fireEvent.click(screen.getByRole("tab", { name: "快照版" }));
    expect(within(screen.getByRole("list", { name: /快照版列表/ })).getByText("24w14a")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: "远古版" }));
    expect(within(screen.getByRole("list", { name: /远古版列表/ })).getByText("b1.7.3")).toBeInTheDocument();
  });

  it("searches version ids", async () => {
    backend();
    renderWithState(<DownloadPage page={page} />);
    await screen.findByText("最新正式版");

    fireEvent.change(screen.getByLabelText("搜索版本"), { target: { value: "1.20" } });

    const list = screen.getByRole("list", { name: /正式版列表，共 1 个/ });
    expect(within(list).getByText("1.20.4")).toBeInTheDocument();
    expect(within(list).queryByText("1.21.1")).not.toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("搜索版本"), { target: { value: "9.9" } });
    expect(screen.getByRole("heading", { name: "没有匹配的版本" })).toBeInTheDocument();
  });

  it("marks installed versions and versions that cannot be launched", async () => {
    backend({}, [survival]);
    renderWithState(<DownloadPage page={page} />);
    await screen.findByText("最新正式版");

    expect(screen.getByText("已安装")).toBeInTheDocument();
    expect(screen.getByText("暂不支持")).toBeInTheDocument();
  });

  it("explains a catalog failure and retries", async () => {
    let attempts = 0;
    backend({
      list_minecraft_versions: () => {
        attempts += 1;
        if (attempts === 1) {
          throw "cannot retrieve Mojang version catalog";
        }
        return catalog;
      },
    });
    renderWithState(<DownloadPage page={page} />);

    expect(await screen.findByRole("alert")).toHaveTextContent("cannot retrieve Mojang version catalog");
    fireEvent.click(screen.getByRole("button", { name: "重新读取" }));

    expect(await screen.findByText("最新正式版")).toBeInTheDocument();
  });

  it("only reads the catalog once while switching pages back and forth", async () => {
    backend();
    const first = renderWithState(<DownloadPage page={page} />);
    await screen.findByText("最新正式版");
    first.unmount();

    renderWithState(<DownloadPage page={page} />);
    await screen.findByText("最新正式版");

    expect(vi.mocked(invoke).mock.calls.filter(([name]) => name === "list_minecraft_versions")).toHaveLength(1);
  });

  it("explains that the browser preview cannot read the catalog", () => {
    pretendBrowser();
    renderWithState(<DownloadPage page={page} />);

    expect(screen.getByText(/浏览器预览无法读取 Mojang 官方版本目录/)).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe("DownloadPage version detail", () => {
  it("shows the version's metadata and goes back to the list", async () => {
    backend();
    renderWithState(<DownloadPage page={page} />);
    await screen.findByText("最新正式版");

    await openVersion("1.20.4");

    expect(screen.getByRole("heading", { level: 1, name: "1.20.4" })).toBeInTheDocument();
    const meta = screen.getByLabelText("版本信息");
    expect(within(meta).getByText("piston-meta.mojang.com")).toBeInTheDocument();
    expect(within(meta).getByText("0123456789ab…")).toBeInTheDocument();
    expect(within(meta).getByText("支持安装与启动")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "返回版本列表" }));
    expect(await screen.findByText("最新正式版")).toBeInTheDocument();
  });

  it("refuses to install a version the launcher cannot start and says why", async () => {
    backend();
    renderWithState(<DownloadPage page={page} />);
    await screen.findByText("最新正式版");

    await openVersion("1.12.2");

    expect(screen.getByText(/1\.13 之前的版本使用旧的启动参数格式/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "安装" })).toBeDisabled();
  });

  it("installs with one button, using the version number as the default instance name", async () => {
    const install = pendingInstall();
    backend({ install_vanilla_version: install.handler });
    renderWithState(<DownloadPage page={page} />);
    await screen.findByText("最新正式版");
    await openVersion("1.21.1");

    expect(screen.getByLabelText(/实例名称/)).toHaveValue("1.21.1");
    fireEvent.click(screen.getByRole("button", { name: "安装" }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("install_vanilla_version", { version: "1.21.1", name: "1.21.1" }),
    );
    expect(await screen.findByText("正在安装 1.21.1")).toBeInTheDocument();
    // 不再有「缓存」「下载」「创建」三个分开的按钮
    expect(screen.queryByRole("button", { name: /缓存/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /创建/ })).not.toBeInTheDocument();
    install.resolve(created);
  });

  it("shows live progress by stage, then the finished instance", async () => {
    const install = pendingInstall();
    let listings = 0;
    backend({
      install_vanilla_version: install.handler,
      list_instances: () => {
        listings += 1;
        return { instances: listings === 1 ? [] : [created], skipped: [] };
      },
    });
    renderWithState(<DownloadPage page={page} />);
    await screen.findByText("最新正式版");
    await openVersion("1.21.1");
    fireEvent.click(screen.getByRole("button", { name: "安装" }));
    await screen.findByText("正在安装 1.21.1");

    await act(async () => {
      progressHandler({
        payload: {
          version: "1.21.1",
          stage: "assets",
          filesDone: 600,
          filesTotal: 1200,
          bytesDone: 50 * 1024 * 1024,
          bytesTotal: 100 * 1024 * 1024,
        },
      });
    });

    expect(screen.getByText("50.0 MB / 100 MB")).toBeInTheDocument();
    expect(screen.getByText("文件 600 / 1200")).toBeInTheDocument();
    expect(screen.getByText("下载游戏资源", { selector: ".install-step-label" }).closest("li")).toHaveAttribute(
      "aria-current",
      "step",
    );
    expect(screen.getByText(/^\d+%$/)).toBeInTheDocument();

    await act(async () => {
      install.resolve(created);
    });

    expect(await screen.findByText("安装完成")).toBeInTheDocument();
    expect(screen.getByText(/实例「1\.21\.1」已创建/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "查看实例" })).toBeInTheDocument();
    // 完成后实例列表被刷新
    await waitFor(() => expect(listings).toBeGreaterThanOrEqual(2));
  });

  it("cancels a running installation", async () => {
    const install = pendingInstall();
    backend({ install_vanilla_version: install.handler, cancel_vanilla_install: () => true });
    renderWithState(<DownloadPage page={page} />);
    await screen.findByText("最新正式版");
    await openVersion("1.21.1");
    fireEvent.click(screen.getByRole("button", { name: "安装" }));
    await screen.findByText("正在安装 1.21.1");

    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(invoke).toHaveBeenCalledWith("cancel_vanilla_install");

    await act(async () => {
      install.reject("cancelled");
    });
    expect(await screen.findByText(/已取消 1\.21\.1 的安装，已下载的文件会保留/)).toBeInTheDocument();
  });

  it("reports a failed installation with the reason and lets the user retry", async () => {
    let calls = 0;
    backend({
      install_vanilla_version: () => {
        calls += 1;
        if (calls === 1) {
          throw "2 of 9 files failed to download or verify (HTTP 503 x2)";
        }
        return created;
      },
    });
    renderWithState(<DownloadPage page={page} />);
    await screen.findByText("最新正式版");
    await openVersion("1.21.1");
    fireEvent.click(screen.getByRole("button", { name: "安装" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("HTTP 503 x2");
    fireEvent.click(screen.getByRole("button", { name: "重试" }));

    expect(await screen.findByText("安装完成")).toBeInTheDocument();
    expect(calls).toBe(2);
  });

  it("will not start an install with a name that is already taken", async () => {
    backend({ install_vanilla_version: () => created }, [survival]);
    renderWithState(<DownloadPage page={page} />);
    await screen.findByText("最新正式版");
    await openVersion("1.21.1");

    fireEvent.change(screen.getByLabelText(/实例名称/), { target: { value: "survival" } });
    fireEvent.click(screen.getByRole("button", { name: "安装" }));

    expect(await screen.findByText("已经有同名实例了")).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("install_vanilla_version", expect.anything());
  });

  it("suggests a free name when the version is already installed", async () => {
    backend({}, [{ ...survival, name: "1.20.4" }]);
    renderWithState(<DownloadPage page={page} />);
    await screen.findByText("最新正式版");
    await waitFor(() => expect(screen.getByText("已安装")).toBeInTheDocument());

    await openVersion("1.20.4");

    expect(screen.getByLabelText(/实例名称/)).toHaveValue("1.20.4 (2)");
  });

  it("blocks a second installation while one is running", async () => {
    const install = pendingInstall();
    backend({ install_vanilla_version: install.handler });
    renderWithState(<DownloadPage page={page} />);
    await screen.findByText("最新正式版");
    await openVersion("1.21.1");
    fireEvent.click(screen.getByRole("button", { name: "安装" }));
    await screen.findByText("正在安装 1.21.1");

    fireEvent.click(screen.getByRole("button", { name: "返回版本列表" }));
    await screen.findByText("最新正式版");
    await openVersion("1.20.4");

    expect(screen.getByText(/正在安装 1\.21\.1，完成后才能安装其他版本/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "安装" })).toBeDisabled();
    install.resolve(created);
  });
});
