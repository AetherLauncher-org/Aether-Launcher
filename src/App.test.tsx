import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { accountBookStorageKey } from "./lib/accounts";
import { pretendBrowser, pretendDesktop, routeInvoke } from "./test/support";

const windowApi = {
  minimize: vi.fn(),
  toggleMaximize: vi.fn(),
  close: vi.fn(),
  setTheme: vi.fn(),
  isMaximized: vi.fn(),
  onResized: vi.fn(),
};

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => windowApi }));

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  for (const mock of Object.values(windowApi)) {
    mock.mockReset();
  }
  windowApi.isMaximized.mockResolvedValue(false);
  windowApi.onResized.mockResolvedValue(() => undefined);
  delete document.documentElement.dataset.theme;
  delete document.documentElement.dataset.motion;
  delete document.documentElement.dataset.material;
});

afterEach(() => {
  pretendBrowser();
});

describe("Aether Launcher shell", () => {
  it("opens on the home page with the first-instance call to action", () => {
    render(<App />);

    expect(screen.getByRole("button", { name: "主页" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("heading", { name: "开始你的第一个实例" })).toBeInTheDocument();
    expect(screen.getByText(/浏览器预览里没有本机实例/)).toBeInTheDocument();
  });

  it("has a real logo, not a letter in a box", () => {
    render(<App />);

    expect(screen.getByRole("img", { name: "Aether Launcher" })).toBeInTheDocument();
    expect(screen.queryByText("A", { selector: ".brand-mark" })).not.toBeInTheDocument();
  });

  // [导航里的按钮名, 页面标题, 页面里一定有的一句话]；账户页的入口是右边的当前账户，不是「账户」按钮
  it.each([
    ["实例", "实例", "还没有实例"],
    ["下载", "下载", "浏览器预览无法读取 Mojang 官方版本目录"],
    ["还没有账户", "账户", "还没有微软账户"],
  ])("switches to the %s page from the navigation", (button, title, landmark) => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: button }));

    expect(screen.getByRole("heading", { level: 1, name: title })).toBeInTheDocument();
    expect(screen.getAllByText(new RegExp(landmark)).length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: button })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("button", { name: "主页" })).not.toHaveAttribute("aria-current");
  });

  it("has no separate accounts button: the current account in the bar is the way in", () => {
    render(<App />);

    expect(screen.queryByRole("button", { name: "账户" })).not.toBeInTheDocument();
  });

  it("returns to the home page", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "实例" }));
    fireEvent.click(screen.getByRole("button", { name: "主页" }));

    expect(screen.getByRole("heading", { name: "开始你的第一个实例" })).toBeInTheDocument();
  });

  it("does not list a resource library page that has no data behind it", () => {
    render(<App />);

    expect(screen.queryByRole("button", { name: "资源库" })).not.toBeInTheDocument();
  });

  it("shows the current account in the navigation and jumps to the accounts page", () => {
    window.localStorage.setItem(
      accountBookStorageKey,
      JSON.stringify({
        accounts: [{ id: "offline-1", kind: "offline", name: "Steve", uuidMode: "standard", uuid: null }],
        activeId: "offline-1",
      }),
    );
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "当前账户 Steve" }));

    expect(screen.getByRole("heading", { level: 1, name: "账户" })).toBeInTheDocument();
  });
});

describe("navigation bar", () => {
  it("sits above the page, not beside it", () => {
    const { container } = render(<App />);
    const bar = container.querySelector("nav.navbar");
    const content = container.querySelector("main.content-layer");

    expect(bar).not.toBeNull();
    expect(content).not.toBeNull();
    // the bar comes before the page in the same column (the column itself is set in shell.css)
    expect(bar?.compareDocumentPosition(content as Node) ?? 0).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    expect(bar?.parentElement).toBe(content?.parentElement);
    expect(bar).toHaveAttribute("aria-label", "主要导航");
  });

  it("lists the main pages in order, then the account and settings on the right", () => {
    const { container } = render(<App />);

    const names = Array.from(container.querySelectorAll("nav.navbar .nav-item")).map((item) =>
      item.getAttribute("aria-label"),
    );

    expect(names).toEqual(["主页", "实例", "下载", "还没有账户", "设置"]);
  });

  it("has nothing to collapse any more", () => {
    render(<App />);

    expect(screen.queryByRole("button", { name: "收起导航" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "展开导航" })).not.toBeInTheDocument();
  });

  it("keeps every page reachable and marks the current one", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "下载" }));

    expect(screen.getByRole("heading", { level: 1, name: "下载" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "下载" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("button", { name: "主页" })).not.toHaveAttribute("aria-current");
  });

  it("names every item in its title, for when only the icons fit", () => {
    render(<App />);

    expect(screen.getByRole("button", { name: "实例" })).toHaveAttribute("title", "实例");
    expect(screen.getByRole("button", { name: "设置" })).toHaveAttribute("title", "设置");
  });
});

describe("window chrome", () => {
  it("draws no window buttons in the browser preview", () => {
    render(<App />);

    expect(screen.queryByRole("button", { name: "关闭" })).not.toBeInTheDocument();
    expect(document.querySelector("header.titlebar")).toHaveAttribute("data-tauri-drag-region");
  });

  it("draws its own minimize, maximize and close buttons in the desktop app", async () => {
    pretendDesktop();
    routeInvoke(vi.mocked(invoke), {
      application_info: () => ({ name: "Aether Launcher", version: "0.1.0", coreStatus: "foundation" }),
      list_instances: () => ({ instances: [], skipped: [] }),
      list_java_runtimes: () => [],
    });
    render(<App />);

    fireEvent.click(await screen.findByRole("button", { name: "最小化" }));
    fireEvent.click(screen.getByRole("button", { name: "最大化" }));
    fireEvent.click(screen.getByRole("button", { name: "关闭" }));

    expect(windowApi.minimize).toHaveBeenCalledTimes(1);
    expect(windowApi.toggleMaximize).toHaveBeenCalledTimes(1);
    expect(windowApi.close).toHaveBeenCalledTimes(1);
  });

  it("switches the maximize button to restore when the window is maximized", async () => {
    pretendDesktop();
    windowApi.isMaximized.mockResolvedValue(true);
    routeInvoke(vi.mocked(invoke), {
      application_info: () => ({ name: "Aether Launcher", version: "0.1.0", coreStatus: "foundation" }),
      list_instances: () => ({ instances: [], skipped: [] }),
      list_java_runtimes: () => [],
    });
    render(<App />);

    expect(await screen.findByRole("button", { name: "还原" })).toBeInTheDocument();
  });

  it("keeps the window material in step with the interface theme", async () => {
    pretendDesktop();
    routeInvoke(vi.mocked(invoke), {
      application_info: () => ({ name: "Aether Launcher", version: "0.1.0", coreStatus: "foundation" }),
      list_instances: () => ({ instances: [], skipped: [] }),
      list_java_runtimes: () => [],
    });
    render(<App />);

    await waitFor(() => expect(windowApi.setTheme).toHaveBeenCalledWith("dark"));
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    fireEvent.click(screen.getByRole("radio", { name: "浅色" }));

    await waitFor(() => expect(windowApi.setTheme).toHaveBeenCalledWith("light"));
  });
});

describe("home greeting", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("follows the local time of day", () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date(2026, 9, 23, 9, 0));

    render(<App />);

    expect(screen.getByRole("heading", { level: 1, name: "早上好" })).toBeInTheDocument();
  });
});

describe("settings", () => {
  it("shows the appearance options with dark selected by default", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "设置" }));

    expect(screen.getByRole("heading", { level: 1, name: "设置" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "深色" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "浅色" })).not.toBeChecked();
    expect(screen.getByRole("heading", { name: "Java 运行时" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "内存" })).toBeInTheDocument();
  });

  it("switches the colour scheme and remembers the choice", () => {
    render(<App />);
    expect(document.documentElement.dataset.theme).toBe("dark");

    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    fireEvent.click(screen.getByRole("radio", { name: "浅色" }));

    expect(document.documentElement.dataset.theme).toBe("light");
    expect(window.localStorage.getItem("aether.theme")).toBe("light");
  });

  it("restores a saved colour scheme on startup", () => {
    window.localStorage.setItem("aether.theme", "light");

    render(<App />);

    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("turns interface animation off and on, and remembers it", () => {
    render(<App />);
    expect(document.documentElement.dataset.motion).toBe("full");

    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    fireEvent.click(screen.getByRole("switch", { name: "界面动画" }));

    expect(document.documentElement.dataset.motion).toBe("reduced");
    expect(window.localStorage.getItem("aether.motion")).toBe("reduced");

    fireEvent.click(screen.getByRole("switch", { name: "界面动画" }));
    expect(document.documentElement.dataset.motion).toBe("full");
  });

  it("restores the animation preference on startup", () => {
    window.localStorage.setItem("aether.motion", "reduced");

    render(<App />);

    expect(document.documentElement.dataset.motion).toBe("reduced");
  });

  it("paints a solid window background when there is no Mica", () => {
    render(<App />);

    expect(document.documentElement.dataset.material).toBe("solid");
  });
});
