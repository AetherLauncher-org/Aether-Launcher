import { FluentProvider, webLightTheme } from "@fluentui/react-components";
import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { SettingsPage } from "./SettingsPage";
import type { MotionPreference } from "../lib/motion";
import type { ThemePreference } from "../lib/theme";
import type { PageDefinition } from "../navigation";

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

  it("shows the memory section with the default value as the only memory control", () => {
    renderSettings();

    expect(screen.getByRole("heading", { name: "内存" })).toBeInTheDocument();
    expect(screen.getByLabelText("内存上限（MiB）")).toHaveValue(4096);
    expect(screen.getByText("当前设置：4 GiB")).toBeInTheDocument();
  });

  it("clamps an out-of-range memory value on blur", () => {
    renderSettings();

    const input = screen.getByLabelText("内存上限（MiB）");
    fireEvent.change(input, { target: { value: "999999999" } });
    fireEvent.blur(input);

    expect(input).toHaveValue(131072);
    expect(window.localStorage.getItem("aether.memoryMiB")).toBe("131072");
  });

  it("pulls a tiny memory value up to the floor", () => {
    renderSettings();

    const input = screen.getByLabelText("内存上限（MiB）");
    fireEvent.change(input, { target: { value: "1" } });
    fireEvent.blur(input);

    expect(input).toHaveValue(512);
  });

  it("persists a valid memory value", () => {
    renderSettings();

    const input = screen.getByLabelText("内存上限（MiB）");
    fireEvent.change(input, { target: { value: "8192" } });
    fireEvent.blur(input);

    expect(window.localStorage.getItem("aether.memoryMiB")).toBe("8192");
    expect(screen.getByText("当前设置：8 GiB")).toBeInTheDocument();
  });

  it("restores a stored memory value", () => {
    window.localStorage.setItem("aether.memoryMiB", "12288");

    renderSettings();

    expect(screen.getByLabelText("内存上限（MiB）")).toHaveValue(12288);
  });
});
