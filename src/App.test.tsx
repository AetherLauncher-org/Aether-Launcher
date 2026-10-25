import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import App from "./App";

describe("Aether Launcher shell", () => {
  it("shows the import-first empty state", () => {
    render(<App />);

    expect(
      screen.getByRole("heading", { name: "尚未导入实例" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "导入实例" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "启动" })).toBeDisabled();
  });

  it("opens on the home page", () => {
    render(<App />);

    expect(screen.getByRole("button", { name: "主页" })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  it.each([
    ["实例", "还没有实例"],
    ["资源库", "资源来源校验完成后可用"],
    ["账户", "离线账户"],
  ])("switches to the %s page from the sidebar", (label, landmark) => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: label }));

    expect(
      screen.getByRole("heading", { level: 1, name: label }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: landmark }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: label })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(screen.getByRole("button", { name: "主页" })).not.toHaveAttribute(
      "aria-current",
    );
  });

  it("returns to the home page", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "实例" }));
    fireEvent.click(screen.getByRole("button", { name: "主页" }));

    expect(
      screen.getByRole("heading", { name: "尚未导入实例" }),
    ).toBeInTheDocument();
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

    expect(
      screen.getByRole("heading", { level: 1, name: "早上好" }),
    ).toBeInTheDocument();
  });
});

describe("settings", () => {
  it("shows the appearance options with dark selected by default", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "设置" }));

    expect(
      screen.getByRole("heading", { level: 1, name: "设置" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "深色" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "浅色" })).not.toBeChecked();
    expect(
      screen.getByRole("heading", { name: "Java 运行时" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "内存" })).toBeInTheDocument();
  });

  it("switches the colour scheme and remembers the choice", () => {
    const { container } = render(<App />);
    const shell = container.querySelector(".app-shell");
    expect(shell).toHaveAttribute("data-theme", "dark");

    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    fireEvent.click(screen.getByRole("radio", { name: "浅色" }));

    expect(shell).toHaveAttribute("data-theme", "light");
    expect(window.localStorage.getItem("aether.theme")).toBe("light");
  });

  it("restores a saved colour scheme on startup", () => {
    window.localStorage.setItem("aether.theme", "light");

    const { container } = render(<App />);

    expect(container.querySelector(".app-shell")).toHaveAttribute(
      "data-theme",
      "light",
    );
  });
});
