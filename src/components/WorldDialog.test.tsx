import { invoke } from "@tauri-apps/api/core";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { pretendBrowser, pretendDesktop, renderWithState, routeInvoke } from "../test/support";
import { WorldDialog } from "./WorldDialog";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));

const pack = (over: Record<string, unknown> = {}) => ({
  name: "Better Trading.zip",
  isFolder: false,
  sizeBytes: 48_000,
  description: "Villagers trade more fairly",
  packFormat: 48,
  state: "enabled",
  ...over,
});

const world = {
  name: "My World",
  levelName: "My World",
  gameVersion: "1.21.1",
  dataVersion: 3955,
  gameMode: "creative",
  hardcore: false,
  difficulty: "hard",
  lastPlayed: 1_761_900_000,
  sizeBytes: 84_000_000,
  icon: null,
  datapacks: [
    pack(),
    pack({ name: "Old.zip", description: null, packFormat: null, state: "disabled" }),
    pack({ name: "fresh", isFolder: true, description: "A folder pack", packFormat: 41, state: "unknown" }),
  ],
  datapacksTruncated: false,
  unreadable: null,
};

type Calls = [string, unknown][];
const calls = (name: string): Calls =>
  vi.mocked(invoke).mock.calls.filter(([command]) => command === name) as Calls;

function show(locked = false, details: unknown = world) {
  routeInvoke(vi.mocked(invoke), {
    get_world_details: () => details,
    remove_world_datapack: () => null,
    open_world_folder: () => null,
  });
  const onClose = vi.fn();
  const onChanged = vi.fn();
  renderWithState(
    <WorldDialog instanceId="i1" world="My World" locked={locked} onClose={onClose} onChanged={onChanged} />,
  );
  return { onClose, onChanged };
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  pretendDesktop();
});

afterEach(() => {
  pretendBrowser();
});

describe("world dialog", () => {
  it("asks for the world by folder name and shows what level.dat says", async () => {
    show();

    expect(await screen.findByText("创造")).toBeInTheDocument();
    expect(screen.getByText("困难")).toBeInTheDocument();
    expect(screen.getByText("1.21.1")).toBeInTheDocument();
    expect(calls("get_world_details")[0][1]).toEqual({ id: "i1", world: "My World" });
  });

  it("lists each data pack with its state, description and format", async () => {
    show();

    expect(await screen.findByText("Villagers trade more fairly")).toBeInTheDocument();
    expect(screen.getByText("已启用")).toBeInTheDocument();
    expect(screen.getByText("已停用")).toBeInTheDocument();
    expect(screen.getByText("未知")).toBeInTheDocument();
    expect(screen.getByText("pack_format 48")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "数据包（3）" })).toBeInTheDocument();
  });

  it("says so when level.dat could not be read, and still lists the packs", async () => {
    show(false, { ...world, levelName: null, gameVersion: null, unreadable: "level.dat: damaged" });

    expect(await screen.findByText(/没能读懂这个世界的 level.dat/)).toBeInTheDocument();
    expect(screen.getByText("Old.zip")).toBeInTheDocument();
  });

  it("removes a data pack only after confirming, by its plain file name", async () => {
    const { onChanged } = show();
    fireEvent.click(await screen.findByRole("button", { name: "删除数据包 Old.zip" }));
    expect(calls("remove_world_datapack")).toHaveLength(0);

    const confirm = await screen.findByRole("alertdialog");
    fireEvent.click(within(confirm).getByRole("button", { name: "删除" }));

    await waitFor(() => expect(calls("remove_world_datapack")).toHaveLength(1));
    expect(calls("remove_world_datapack")[0][1]).toEqual({ id: "i1", world: "My World", name: "Old.zip" });
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
  });

  it("cannot remove anything while the game runs", async () => {
    show(true);

    const button = await screen.findByRole("button", { name: "删除数据包 Old.zip" });

    expect(button).toBeDisabled();
  });

  it("shows the reason when the backend refuses to remove", async () => {
    routeInvoke(vi.mocked(invoke), {
      get_world_details: () => world,
      remove_world_datapack: () => {
        throw "stop the game before removing a data pack";
      },
    });
    renderWithState(
      <WorldDialog instanceId="i1" world="My World" locked={false} onClose={vi.fn()} onChanged={vi.fn()} />,
    );
    fireEvent.click(await screen.findByRole("button", { name: "删除数据包 Old.zip" }));
    const confirm = await screen.findByRole("alertdialog");
    fireEvent.click(within(confirm).getByRole("button", { name: "删除" }));

    expect(await within(confirm).findByRole("alert")).toHaveTextContent("stop the game");
  });

  it("opens the world folder by name", async () => {
    show();

    fireEvent.click(await screen.findByRole("button", { name: "打开世界文件夹" }));

    await waitFor(() => expect(calls("open_world_folder")).toHaveLength(1));
    expect(calls("open_world_folder")[0][1]).toEqual({ id: "i1", world: "My World" });
  });

  it("asks for nothing while it is closed", () => {
    routeInvoke(vi.mocked(invoke), { get_world_details: () => world });
    renderWithState(
      <WorldDialog instanceId="i1" world={null} locked={false} onClose={vi.fn()} onChanged={vi.fn()} />,
    );

    expect(calls("get_world_details")).toHaveLength(0);
  });
});
