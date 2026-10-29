import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ImportInstancesDialog } from "./ImportInstancesDialog";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

function enableTauri() {
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {},
    configurable: true,
  });
}

/** 按命令名分发 mock 返回值。 */
function mockCommands(commands: Record<string, unknown>) {
  vi.mocked(invoke).mockImplementation(((cmd: string) => {
    const result = commands[cmd];
    if (result instanceof Error) {
      return Promise.reject(result.message);
    }
    return Promise.resolve(result);
  }) as unknown as typeof invoke);
}

function renderDialog() {
  const onImported = vi.fn();
  render(
    <ImportInstancesDialog
      open
      onOpenChange={() => {}}
      onImported={onImported}
    />,
  );
  return onImported;
}

const folderSource = {
  token: "t1",
  sourceKind: "folder",
  sourcePath: "D:\\MC",
  candidates: [{ id: "1.20.1", displayName: "1.20.1" }],
};

const importedInstance = {
  id: "a",
  name: "1.20.1",
  version: "1.20.1",
  state: "incomplete",
};

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

afterEach(() => {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
});

describe("ImportInstancesDialog", () => {
  it("offers both import branches", () => {
    enableTauri();
    mockCommands({});

    renderDialog();

    expect(screen.getByRole("tab", { name: "从文件夹导入" })).toBeInTheDocument();
    expect(
      screen.getByRole("tab", { name: "从启动器导入" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "选择 .minecraft 文件夹" }),
    ).toBeInTheDocument();
  });

  it("imports the versions of a picked folder", async () => {
    enableTauri();
    mockCommands({
      pick_import_folder: folderSource,
      import_versions: { imported: [importedInstance], skipped: [] },
    });
    const onImported = renderDialog();

    fireEvent.click(
      screen.getByRole("button", { name: "选择 .minecraft 文件夹" }),
    );
    expect(await screen.findByText("D:\\MC")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /导入选中/ }));

    await waitFor(() => expect(onImported).toHaveBeenCalledTimes(1));
    expect(onImported.mock.calls[0][0]).toEqual([importedInstance]);
    expect(invoke).toHaveBeenCalledWith("import_versions", {
      token: "t1",
      ids: ["1.20.1"],
    });
    expect(await screen.findByText("已导入 1 个实例")).toBeInTheDocument();
  });

  it("does nothing when the folder picker is dismissed", async () => {
    enableTauri();
    mockCommands({ pick_import_folder: null });

    renderDialog();
    fireEvent.click(
      screen.getByRole("button", { name: "选择 .minecraft 文件夹" }),
    );

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("pick_import_folder"),
    );
    expect(screen.getByRole("button", { name: /导入选中/ })).toBeDisabled();
  });

  it("reports a scan failure without importing", async () => {
    enableTauri();
    mockCommands({
      pick_import_folder: new Error("the selected folder is not a readable directory"),
    });

    renderDialog();
    fireEvent.click(
      screen.getByRole("button", { name: "选择 .minecraft 文件夹" }),
    );

    expect(
      await screen.findByText("the selected folder is not a readable directory"),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /导入选中/ })).toBeDisabled();
  });

  it("lists the game directories a detected launcher points at", async () => {
    enableTauri();
    mockCommands({
      detect_launcher_installations: [
        {
          kind: "pcl2",
          launcherRoot: "D:\\PCL2",
          sources: [
            {
              token: "t2",
              sourceKind: "pcl2",
              sourcePath: "D:\\PCL2\\.minecraft",
              candidates: [{ id: "1.21", displayName: "1.21" }],
            },
          ],
        },
      ],
    });

    renderDialog();
    fireEvent.click(screen.getByRole("tab", { name: "从启动器导入" }));

    expect(await screen.findByText("D:\\PCL2")).toBeInTheDocument();
    expect(screen.getByText("D:\\PCL2\\.minecraft")).toBeInTheDocument();
    expect(screen.getByText("PCL2")).toBeInTheDocument();
    expect(screen.getByText("1 个版本")).toBeInTheDocument();
  });

  it("labels a Prism launcher and counts its instances", async () => {
    enableTauri();
    mockCommands({
      detect_launcher_installations: [
        {
          kind: "prism",
          launcherRoot: "E:\\MultiMC",
          sources: [
            {
              token: "t3",
              sourceKind: "prism",
              sourcePath: "E:\\MultiMC",
              candidates: [
                { id: "ranked", displayName: "Ranked（1.16.1 + Fabric 0.18.4）" },
              ],
            },
          ],
        },
      ],
    });

    renderDialog();
    fireEvent.click(screen.getByRole("tab", { name: "从启动器导入" }));

    expect(await screen.findByText("Prism / MultiMC")).toBeInTheDocument();
    expect(screen.getByText("1 个实例")).toBeInTheDocument();
  });

  it("explains when no launcher was found", async () => {
    enableTauri();
    mockCommands({ detect_launcher_installations: [] });

    renderDialog();
    fireEvent.click(screen.getByRole("tab", { name: "从启动器导入" }));

    expect(await screen.findByText(/没有自动找到启动器/)).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "手动选择启动器文件夹" }),
    ).toBeInTheDocument();
  });

  it("translates the skip reasons of a partial import", async () => {
    enableTauri();
    mockCommands({
      pick_import_folder: {
        ...folderSource,
        candidates: [{ id: "1.21", displayName: "1.21" }],
      },
      import_versions: {
        imported: [],
        skipped: [{ id: "1.21", reason: "missing-version-json" }],
      },
    });
    const onImported = renderDialog();

    fireEvent.click(
      screen.getByRole("button", { name: "选择 .minecraft 文件夹" }),
    );
    await screen.findByText("D:\\MC");
    fireEvent.click(screen.getByRole("button", { name: /导入选中/ }));

    expect(await screen.findByText(/缺少版本 JSON 文件/)).toBeInTheDocument();
    expect(
      screen.getByText("没有导入成功，1 个版本被跳过"),
    ).toBeInTheDocument();
    expect(onImported).not.toHaveBeenCalled();
  });

  it("imports in one step and sends only the token and the ticked ids", async () => {
    enableTauri();
    mockCommands({
      pick_import_folder: folderSource,
      import_versions: { imported: [importedInstance], skipped: [] },
    });

    const onImported = renderDialog();
    fireEvent.click(
      screen.getByRole("button", { name: "选择 .minecraft 文件夹" }),
    );
    await screen.findByText("D:\\MC");
    fireEvent.click(screen.getByRole("button", { name: /导入选中/ }));

    expect(await screen.findByText("已导入 1 个实例")).toBeInTheDocument();
    expect(onImported).toHaveBeenCalledWith([importedInstance]);
    expect(invoke).toHaveBeenCalledWith("import_versions", {
      token: folderSource.token,
      ids: folderSource.candidates.map((candidate) => candidate.id),
    });
    // The files are installed as part of the import, not by a second command.
    expect(
      screen.queryByRole("button", { name: "下载并补齐游戏文件" }),
    ).not.toBeInTheDocument();
  });

  it("tells a Prism user that loader instances are skipped", async () => {
    enableTauri();
    mockCommands({
      detect_launcher_installations: [
        {
          kind: "prism",
          launcherRoot: "E:\\MultiMC",
          sources: [
            {
              token: "t3",
              sourceKind: "prism",
              sourcePath: "E:\\MultiMC",
              candidates: [{ id: "ranked", displayName: "Ranked（1.16.1）" }],
            },
          ],
        },
      ],
      import_versions: {
        imported: [],
        skipped: [{ id: "ranked", reason: "needs-loader" }],
      },
    });

    renderDialog();
    fireEvent.click(screen.getByRole("tab", { name: "从启动器导入" }));
    fireEvent.click(await screen.findByRole("button", { name: /E:\\MultiMC/ }));
    expect(screen.getByText(/带 Fabric、/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /导入选中/ }));

    expect(await screen.findByText(/需要 Fabric \/ Forge/)).toBeInTheDocument();
  });
});
