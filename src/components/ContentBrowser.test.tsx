import { invoke } from "@tauri-apps/api/core";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { pretendBrowser, pretendDesktop, renderWithState, routeInvoke } from "../test/support";
import type { Handlers } from "../test/support";
import { ContentBrowser } from "./ContentBrowser";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));

const survival = { id: "i1", name: "Survival", version: "fabric-loader-0.16.5-1.21.1", state: "ready" };

const hit = (over: Record<string, unknown> = {}) => ({
  projectId: "AANobbMI",
  slug: "sodium",
  title: "Sodium",
  summary: "A fast renderer",
  author: "jellysquid3",
  downloads: 12_000_000,
  projectType: "mod",
  ...over,
});

type Calls = [string, unknown][];
const calls = (name: string): Calls =>
  vi.mocked(invoke).mock.calls.filter(([command]) => command === name) as Calls;
const lastArgs = (name: string): unknown => {
  const all = calls(name);
  return all[all.length - 1]?.[1];
};

function backend(overrides: Handlers = {}, instances: unknown[] = [survival]) {
  routeInvoke(vi.mocked(invoke), {
    list_instances: () => ({ instances, skipped: [] }),
    get_content_target: () => ({ gameVersion: "1.21.1", loader: "fabric" }),
    search_content: () => ({ hits: [hit()], total: 1 }),
    install_content: () => ({
      installed: [{ projectId: "AANobbMI", fileName: "sodium-1.0.0.jar", version: "1.0.0" }],
      skipped: [],
    }),
    list_instance_content: () => [
      { name: "My World", sizeBytes: 1, enabled: true, isFolder: true },
      { name: "Creative", sizeBytes: 1, enabled: true, isFolder: true },
    ],
    open_content_project: () => null,
    ...overrides,
  });
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  pretendDesktop();
});

afterEach(() => {
  pretendBrowser();
});

describe("content browser", () => {
  it("searches for mods narrowed to the instance's game version and loader", async () => {
    backend();
    renderWithState(<ContentBrowser type="mod" />);

    expect(await screen.findByText("Sodium")).toBeInTheDocument();
    expect(await screen.findByText("1.21.1 · Fabric")).toBeInTheDocument();
    expect(lastArgs("search_content")).toEqual({
      text: "",
      projectType: "mod",
      gameVersion: "1.21.1",
      loader: "fabric",
      sort: "downloads",
      offset: 0,
    });
  });

  it("can search without the instance's filters", async () => {
    backend();
    renderWithState(<ContentBrowser type="mod" />);
    await screen.findByText("Sodium");

    fireEvent.click(screen.getByRole("switch", { name: "只看适合这个实例的" }));

    await waitFor(() =>
      expect(lastArgs("search_content")).toMatchObject({ gameVersion: null, loader: null }),
    );
  });

  it("searches for the typed words when asked, not on every key", async () => {
    backend();
    renderWithState(<ContentBrowser type="mod" />);
    await screen.findByText("Sodium");
    const before = calls("search_content").length;

    fireEvent.change(screen.getByRole("searchbox", { name: "搜索内容" }), { target: { value: "lithium" } });
    expect(calls("search_content")).toHaveLength(before);
    fireEvent.click(screen.getByRole("button", { name: "搜索" }));

    await waitFor(() => expect(lastArgs("search_content")).toMatchObject({ text: "lithium" }));
  });

  it("drops the loader for types it does not apply to", async () => {
    backend();
    renderWithState(<ContentBrowser type="resourcepack" />);

    await screen.findByText("Sodium");

    expect(lastArgs("search_content")).toMatchObject({
      projectType: "resourcepack",
      loader: null,
      gameVersion: "1.21.1",
    });
  });

  it("installs by instance id and project id only, and says what was installed", async () => {
    backend();
    renderWithState(<ContentBrowser type="mod" />);

    fireEvent.click(await screen.findByRole("button", { name: "安装 Sodium" }));

    await waitFor(() => expect(calls("install_content")).toHaveLength(1));
    expect(calls("install_content")[0][1]).toEqual({
      id: "i1",
      projectId: "AANobbMI",
      projectType: "mod",
      world: null,
      withDependencies: true,
    });
    expect(await screen.findByText(/已装 Sodium：sodium-1.0.0.jar/)).toBeInTheDocument();
  });

  it("reports required dependencies that could not be installed", async () => {
    backend({
      install_content: () => ({
        installed: [{ projectId: "AANobbMI", fileName: "sodium-1.0.0.jar", version: "1.0.0" }],
        skipped: [{ projectId: "P7dR8mSH", reason: "no version of this project fits Minecraft 1.21.1" }],
      }),
    });
    renderWithState(<ContentBrowser type="mod" />);

    fireEvent.click(await screen.findByRole("button", { name: "安装 Sodium" }));

    expect(await screen.findByText(/1 个前置没能装上/)).toBeInTheDocument();
  });

  it("shows why an install was refused", async () => {
    backend({
      install_content: () => {
        throw "stop the game before installing content";
      },
    });
    renderWithState(<ContentBrowser type="mod" />);

    fireEvent.click(await screen.findByRole("button", { name: "安装 Sodium" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("stop the game before installing content");
  });

  it("does not offer mods for an instance with no mod loader", async () => {
    backend(
      { get_content_target: () => ({ gameVersion: "1.21.1", loader: null }) },
      [{ ...survival, version: "1.21.1" }],
    );
    renderWithState(<ContentBrowser type="mod" />);

    expect(await screen.findByText(/没有模组加载器/)).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: "安装 Sodium" })).toBeDisabled();
  });

  it("installs a data pack into every world that was ticked", async () => {
    backend({ search_content: () => ({ hits: [hit({ title: "Tweaks", projectId: "DDDDDDDD", projectType: "datapack" })], total: 1 }) });
    renderWithState(<ContentBrowser type="datapack" />);

    fireEvent.click(await screen.findByRole("button", { name: "安装 Tweaks" }));
    const picker = await screen.findByRole("dialog");
    // nothing is chosen yet, so nothing can be installed
    expect(within(picker).getByRole("button", { name: "安装" })).toBeDisabled();
    fireEvent.click(await within(picker).findByRole("checkbox", { name: "My World" }));
    fireEvent.click(within(picker).getByRole("checkbox", { name: "Creative" }));
    fireEvent.click(within(picker).getByRole("button", { name: "安装" }));

    await waitFor(() => expect(calls("install_content")).toHaveLength(2));
    expect(calls("install_content").map(([, args]) => (args as { world: string }).world).sort()).toEqual([
      "Creative",
      "My World",
    ]);
    expect(calls("install_content")[0][1]).toMatchObject({ projectType: "datapack", withDependencies: false });
  });

  it("opens the project page by its slug", async () => {
    backend();
    renderWithState(<ContentBrowser type="mod" />);

    fireEvent.click(await screen.findByRole("button", { name: "在 Modrinth 打开 Sodium" }));

    await waitFor(() => expect(calls("open_content_project")[0]?.[1]).toEqual({ slug: "sodium" }));
  });

  it("asks for an instance first when there is none", async () => {
    backend({}, []);
    renderWithState(<ContentBrowser type="mod" />);

    expect(await screen.findByText("先有一个实例")).toBeInTheDocument();
    expect(calls("search_content")).toHaveLength(0);
  });
});
