import { FluentProvider, webLightTheme } from "@fluentui/react-components";
import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { pretendBrowser, pretendDesktop, routeInvoke } from "../test/support";
import { SettingsPage } from "./SettingsPage";
import { loginMethodStorageKey } from "../lib/loginMethod";
import type { MotionPreference } from "../lib/motion";
import type { ThemePreference } from "../lib/theme";
import type { PageDefinition } from "../navigation";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const page: PageDefinition = {
  id: "settings",
  label: "设置",
  summary: "外观、内存、Java 与下载来源",
  icon: <span />,
  activeIcon: <span />,
};

function renderSettings(preference: ThemePreference = "dark", motion: MotionPreference = "full") {
  const themeChanges: ThemePreference[] = [];
  const motionChanges: MotionPreference[] = [];
  render(
    <FluentProvider theme={webLightTheme}>
      <SettingsPage
        page={page}
        themePreference={preference}
        onThemeChange={(next) => themeChanges.push(next)}
        motion={motion}
        onMotionChange={(next) => motionChanges.push(next)}
        version="0.1.0"
      />
    </FluentProvider>,
  );
  return { themeChanges, motionChanges };
}

beforeEach(() => {
  window.localStorage.clear();
});

afterEach(() => {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
});

describe("SettingsPage", () => {
  it("keeps the appearance section", () => {
    renderSettings();

    expect(screen.getByRole("heading", { name: "外观" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "深色" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "浅色" })).not.toBeChecked();
  });

  it("still reports colour scheme changes", () => {
    const { themeChanges } = renderSettings();

    fireEvent.click(screen.getByRole("radio", { name: "浅色" }));

    expect(themeChanges).toEqual(["light"]);
  });

  it("lets the user choose how to sign in to Microsoft, with the device code as the default", () => {
    renderSettings();

    expect(screen.getByRole("heading", { name: "账户登录" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "设备码（默认）" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "浏览器授权" })).not.toBeChecked();

    fireEvent.click(screen.getByRole("radio", { name: "浏览器授权" }));

    expect(screen.getByRole("radio", { name: "浏览器授权" })).toBeChecked();
    expect(window.localStorage.getItem(loginMethodStorageKey)).toBe("browser");
  });

  it("shows the sign-in method that was saved", () => {
    window.localStorage.setItem(loginMethodStorageKey, "browser");

    renderSettings();

    expect(screen.getByRole("radio", { name: "浏览器授权" })).toBeChecked();
  });

  it("has a switch for interface animation that reflects and changes the preference", () => {
    const { motionChanges } = renderSettings("dark", "full");

    const toggle = screen.getByRole("switch", { name: "界面动画" });
    expect(toggle).toBeChecked();
    fireEvent.click(toggle);

    expect(motionChanges).toEqual(["reduced"]);
  });

  it("shows animation as off when reduced", () => {
    renderSettings("dark", "reduced");

    expect(screen.getByRole("switch", { name: "界面动画" })).not.toBeChecked();
  });

  it("shows the Java section with an empty state outside Tauri, without promising it affects launch", () => {
    renderSettings();

    expect(screen.getByRole("heading", { name: "Java 运行时" })).toBeInTheDocument();
    expect(screen.getByText("暂未检测到 Java")).toBeInTheDocument();
    expect(screen.getByText(/需要桌面版才能扫描本机的 Java 安装/)).toBeInTheDocument();
    expect(screen.getByText(/暂不影响启动/)).toBeInTheDocument();
  });

  it("states the download source and the app version", () => {
    renderSettings();

    expect(screen.getByRole("heading", { name: "下载来源" })).toBeInTheDocument();
    expect(screen.getByText("Mojang 官方")).toBeInTheDocument();
    expect(screen.getByText("版本 0.1.0")).toBeInTheDocument();
  });

  it("offers automatic and custom memory, defaulting to automatic with a read-only slider", () => {
    renderSettings();

    expect(screen.getByRole("heading", { name: "内存" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /自动（推荐/ })).toBeChecked();
    expect(screen.getByRole("radio", { name: "自定义" })).not.toBeChecked();
    expect(screen.getByRole("slider", { name: "游戏内存上限" })).toBeDisabled();
    // 浏览器预览读不到本机内存：用默认值，并如实说明范围是估计的
    expect(screen.getByText(/没能读取本机内存，范围按 16 GiB 估计/)).toBeInTheDocument();
  });

  it("is the only place to change memory, as one slider and no separate number box", () => {
    renderSettings();

    expect(screen.getAllByRole("slider")).toHaveLength(1);
    expect(screen.queryByLabelText("内存上限（MiB）")).not.toBeInTheDocument();
  });

  it("switches to custom, enables the slider and saves what the user picks", () => {
    renderSettings();

    fireEvent.click(screen.getByRole("radio", { name: "自定义" }));
    const slider = screen.getByRole("slider", { name: "游戏内存上限" });
    expect(slider).toBeEnabled();
    fireEvent.change(slider, { target: { value: "8192" } });

    expect(window.localStorage.getItem("aether.memoryMode")).toBe("custom");
    expect(window.localStorage.getItem("aether.memoryMiB")).toBe("8192");
    expect(screen.getByRole("slider", { name: "游戏内存上限" })).toHaveAttribute("aria-valuetext", "8 GiB");
  });

  it("goes back to automatic without losing the custom value", () => {
    renderSettings();
    fireEvent.click(screen.getByRole("radio", { name: "自定义" }));
    fireEvent.change(screen.getByRole("slider", { name: "游戏内存上限" }), { target: { value: "6144" } });

    fireEvent.click(screen.getByRole("radio", { name: /自动（推荐/ }));
    expect(screen.getByRole("slider", { name: "游戏内存上限" })).toBeDisabled();
    expect(window.localStorage.getItem("aether.memoryMode")).toBe("auto");

    fireEvent.click(screen.getByRole("radio", { name: "自定义" }));
    expect(screen.getByRole("slider", { name: "游戏内存上限" })).toHaveAttribute("aria-valuetext", "6 GiB");
  });

  it("restores a value saved before modes existed as a custom setting", () => {
    window.localStorage.setItem("aether.memoryMiB", "12288");

    renderSettings();

    expect(screen.getByRole("radio", { name: "自定义" })).toBeChecked();
    expect(screen.getByRole("slider", { name: "游戏内存上限" })).toHaveAttribute("aria-valuetext", "12 GiB");
  });
});

describe("SettingsPage memory on a real machine", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    pretendDesktop();
    routeInvoke(vi.mocked(invoke), {
      system_memory: () => ({ totalMib: 16_284, availableMib: 2_355 }),
      list_java_runtimes: () => [],
    });
  });

  afterEach(() => {
    pretendBrowser();
  });

  it("sizes the slider from the installed memory and shows used, total and available", async () => {
    renderSettings();

    expect(await screen.findByText("13.6 GiB / 15.9 GiB")).toBeInTheDocument();
    const slider = screen.getByRole("slider", { name: "游戏内存上限" });
    expect(slider).toHaveAttribute("max", "16128");
    expect(slider).toHaveAttribute("min", "512");
    expect(screen.getByText(/（可用 2.3 GiB）/)).toBeInTheDocument();
    expect(screen.queryByText(/没能读取本机内存/)).not.toBeInTheDocument();
  });

  it("recommends from what is free right now when automatic", async () => {
    renderSettings();

    // 可用 2.3 GiB 很少，推荐值落在 2 GiB 的下限
    expect(await screen.findByRole("radio", { name: "自动（推荐，现在约 2 GiB）" })).toBeChecked();
    expect(screen.getByRole("slider", { name: "游戏内存上限" })).toHaveAttribute("aria-valuetext", "2 GiB");
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("warns when the custom value is more than the memory that is free", async () => {
    renderSettings();
    await screen.findByText("13.6 GiB / 15.9 GiB");

    fireEvent.click(screen.getByRole("radio", { name: "自定义" }));
    fireEvent.change(screen.getByRole("slider", { name: "游戏内存上限" }), { target: { value: "9216" } });

    expect(await screen.findByText(/超过了当前可用内存/)).toBeInTheDocument();
    expect(screen.getByText(/9 GiB（可用 2.3 GiB）/)).toBeInTheDocument();

    fireEvent.change(screen.getByRole("slider", { name: "游戏内存上限" }), { target: { value: "2048" } });
    expect(screen.queryByText(/超过了当前可用内存/)).not.toBeInTheDocument();
  });

  it("does not claim a used figure when the platform reports no available memory", async () => {
    routeInvoke(vi.mocked(invoke), {
      system_memory: () => ({ totalMib: 8192, availableMib: null }),
      list_java_runtimes: () => [],
    });
    renderSettings();

    expect(await screen.findByText("— / 8 GiB")).toBeInTheDocument();
    expect(screen.queryByText(/可用/, { selector: ".memory-bar-stat *" })).not.toBeInTheDocument();
  });
});
