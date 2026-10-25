import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { SettingsPage } from "./SettingsPage";
import type { ThemePreference } from "../lib/theme";
import type { PageDefinition } from "../navigation";

const page: PageDefinition = {
  id: "settings",
  label: "设置",
  icon: <span />,
  summary: "外观、Java 运行时、内存与下载来源",
  pendingTitle: "",
  pendingBody: "",
};

function renderSettings(preference: ThemePreference = "dark") {
  const changes: ThemePreference[] = [];
  render(
    <SettingsPage
      page={page}
      themePreference={preference}
      onThemeChange={(next) => changes.push(next)}
    />,
  );
  return changes;
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
    const changes = renderSettings();

    fireEvent.click(screen.getByRole("radio", { name: "浅色" }));

    expect(changes).toEqual(["light"]);
  });

  it("shows the Java section with an empty state outside Tauri", () => {
    renderSettings();

    expect(
      screen.getByRole("heading", { name: "Java 运行时" }),
    ).toBeInTheDocument();
    expect(screen.getByText("暂未检测到 Java")).toBeInTheDocument();
    expect(screen.getByText(/需要桌面版才能扫描本机的 Java 安装/)).toBeInTheDocument();
  });

  it("shows the memory section with the default value", () => {
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
