import { FluentProvider, webLightTheme } from "@fluentui/react-components";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { InstallContext } from "../state/install";
import type { InstallApi, InstallJob } from "../state/install";
import { NavigationContext } from "../state/navigation";
import { TitleBar } from "./TitleBar";

function renderTitleBar(job: InstallJob | null) {
  const navigate = vi.fn();
  const install: InstallApi = { job, start: vi.fn(), cancel: vi.fn(), dismiss: vi.fn() };
  render(
    <FluentProvider theme={webLightTheme}>
      <NavigationContext.Provider value={{ current: "home", navigate }}>
        <InstallContext.Provider value={install}>
          <TitleBar />
        </InstallContext.Provider>
      </NavigationContext.Provider>
    </FluentProvider>,
  );
  return navigate;
}

const running: InstallJob = {
  version: "1.21.1",
  name: "1.21.1",
  status: "running",
  progress: {
    version: "1.21.1",
    stage: "files",
    filesDone: 1,
    filesTotal: 2,
    bytesDone: 50,
    bytesTotal: 100,
  },
  speed: 0,
  instance: null,
  error: null,
};

describe("TitleBar", () => {
  it("is a drag region carrying the brand", () => {
    renderTitleBar(null);

    expect(screen.getByRole("banner")).toHaveAttribute("data-tauri-drag-region");
    expect(screen.getByText("Aether Launcher")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /正在安装/ })).not.toBeInTheDocument();
  });

  it("shows a progress chip while installing and returns to the download page when clicked", () => {
    const navigate = renderTitleBar(running);

    const chip = screen.getByRole("button", { name: /正在安装 1\.21\.1，17%/ });
    expect(chip).toHaveTextContent("安装 1.21.1 · 17%");
    fireEvent.click(chip);

    expect(navigate).toHaveBeenCalledWith("download");
  });

  it("hides the chip once the installation is over", () => {
    renderTitleBar({ ...running, status: "done" });

    expect(screen.queryByRole("button", { name: /正在安装/ })).not.toBeInTheDocument();
  });
});
