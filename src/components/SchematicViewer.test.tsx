import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SchematicView } from "../ipc/schematics";
import type { RendererEvents } from "../lib/schematic/renderer";
import { SchematicViewer } from "./SchematicViewer";

const fake = vi.hoisted(() => {
  class FakeRenderer {
    static instances: FakeRenderer[] = [];
    static failToStart = false;
    events: RendererEvents;
    setKey = vi.fn();
    setShowStats = vi.fn();
    setMouse = vi.fn();
    requestLock = vi.fn();
    releaseLock = vi.fn();
    look = vi.fn();
    scaleSpeed = vi.fn();
    releaseKeys = vi.fn();
    resize = vi.fn();
    dispose = vi.fn();

    constructor(_canvas: unknown, _grid: unknown, events: RendererEvents) {
      if (FakeRenderer.failToStart) {
        throw new Error("no webgl");
      }
      this.events = events;
      FakeRenderer.instances.push(this);
    }
  }
  return { FakeRenderer };
});

vi.mock("../lib/schematic/renderer", () => ({ SchematicRenderer: fake.FakeRenderer }));

function toBase64(runs: [number, number][]): string {
  const bytes = runs.flatMap(([value, count]) => [value & 255, value >> 8, 0, 0, count & 255, (count >> 8) & 255, 0, 0]);
  return btoa(String.fromCharCode(...bytes));
}

/** 3×2×2：一个箱子在 (2,1,1)，其余是石头或空气。 */
const view: SchematicView = {
  fileName: "Vault.litematic",
  format: "litematic",
  name: "Vault",
  author: "Aether",
  description: "",
  dataVersion: 3955,
  size: [3, 2, 2],
  solidBlocks: 6,
  palette: [
    { name: "minecraft:air", properties: [], air: true, count: 6 },
    { name: "minecraft:stone", properties: [], air: false, count: 5 },
    { name: "minecraft:chest", properties: [{ key: "facing", value: "west" }, { key: "type", value: "single" }], air: false, count: 1 },
  ],
  // y=0: stone x5 then air; y=1: air x5 then chest at (2,1,1)
  runs: toBase64([[1, 5], [0, 1], [0, 5], [2, 1]]),
  runCount: 4,
  blockEntities: [
    {
      x: 2,
      y: 1,
      z: 1,
      id: "minecraft:chest",
      snbt: '{\n  id: "minecraft:chest",\n  CustomName: "Loot",\n  Items: []\n}',
      items: [
        { slot: 3, id: "minecraft:diamond_sword", count: 2, data: '{\n  Damage: 7\n}' },
      ],
    },
  ],
  blockEntityTotal: 1,
};

const chestHit = { x: 2, y: 1, z: 1, index: (1 * 2 + 1) * 3 + 2, palette: 2, normal: [0, 1, 0] as [number, number, number], distance: 3 };
const stoneHit = { x: 0, y: 0, z: 0, index: 0, palette: 1, normal: [-1, 0, 0] as [number, number, number], distance: 1 };

function latest() {
  return fake.FakeRenderer.instances[fake.FakeRenderer.instances.length - 1];
}

beforeEach(() => {
  fake.FakeRenderer.instances = [];
  fake.FakeRenderer.failToStart = false;
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
});

afterEach(() => {
  vi.unstubAllGlobals();
});

function open(onClose = vi.fn()) {
  const result = render(<SchematicViewer view={view} onClose={onClose} />);
  return { onClose, ...result };
}

function hover(hit: typeof chestHit | typeof stoneHit | null) {
  act(() => latest().events.onHover(hit));
}

describe("SchematicViewer", () => {
  it("is a modal with the schematic's name, size and block count", () => {
    open();

    const dialog = screen.getByRole("dialog", { name: "Vault 的 3D 预览" });
    expect(within(dialog).getByText("Vault")).toBeInTheDocument();
    expect(within(dialog).getByText(/3×2×2 · 6 个方块 · 数据版本 3955/)).toBeInTheDocument();
    expect(fake.FakeRenderer.instances).toHaveLength(1);
  });

  it("shows nothing until a block is pointed at, then the block's name, id, position, state and face", () => {
    open();
    expect(screen.queryByLabelText("方块信息")).not.toBeInTheDocument();

    hover(chestHit);

    const card = screen.getByLabelText("方块信息");
    expect(within(card).getByText("Chest")).toBeInTheDocument();
    expect(within(card).getByText("minecraft:chest")).toBeInTheDocument();
    expect(within(card).getByText(/x 2\s+y 1\s+z 1/)).toBeInTheDocument();
    expect(within(card).getByText(/面：上/)).toBeInTheDocument();
    expect(within(card).getByText("facing")).toBeInTheDocument();
    expect(within(card).getByText("west")).toBeInTheDocument();
    expect(within(card).getByText("type")).toBeInTheDocument();
  });

  it("shows a container's items and the start of its NBT", () => {
    open();

    hover(chestHit);

    const card = screen.getByLabelText("方块信息");
    expect(within(card).getByText("方块实体：minecraft:chest")).toBeInTheDocument();
    const items = within(card).getByLabelText("容器内容");
    expect(within(items).getByText("Diamond Sword ×2")).toBeInTheDocument();
    expect(within(items).getByText("#3")).toBeInTheDocument();
    expect(within(card).getByText(/CustomName: "Loot"/)).toBeInTheDocument();
  });

  it("a plain block has no block entity section, and pointing away clears the card", () => {
    open();

    hover(stoneHit);
    const card = screen.getByLabelText("方块信息");
    expect(within(card).getByText("Stone")).toBeInTheDocument();
    expect(within(card).queryByText(/方块实体/)).not.toBeInTheDocument();

    hover(null);
    expect(screen.queryByLabelText("方块信息")).not.toBeInTheDocument();
  });

  it("pins the pointed block with F and shows everything about it", () => {
    open();
    hover(chestHit);

    fireEvent.keyDown(window, { code: "KeyF" });

    const inspector = screen.getByLabelText("固定的方块");
    // The block's id, and the block entity's id as a heading.
    expect(within(inspector).getAllByText("minecraft:chest")).toHaveLength(2);
    expect(within(inspector).getByText("2, 1, 1")).toBeInTheDocument();
    expect(within(inspector).getByLabelText("完整 NBT")).toHaveTextContent('CustomName: "Loot"');
    // The item's own data is shown in full here.
    expect(within(inspector).getByText(/Damage: 7/)).toBeInTheDocument();

    // It stays when the pointer moves on, until it is unpinned.
    hover(stoneHit);
    expect(screen.getByLabelText("固定的方块")).toBeInTheDocument();
    fireEvent.click(within(inspector).getByRole("button", { name: "取消固定" }));
    expect(screen.queryByLabelText("固定的方块")).not.toBeInTheDocument();
  });

  it("pinning with nothing pointed at pins nothing", () => {
    open();

    fireEvent.keyDown(window, { code: "KeyF" });

    expect(screen.queryByLabelText("固定的方块")).not.toBeInTheDocument();
  });

  it("sends WASD, Space, Shift and Ctrl to the camera, and stops them on release", () => {
    open();
    const renderer = latest();

    const pairs: [string, string][] = [
      ["KeyW", "forward"],
      ["KeyS", "back"],
      ["KeyA", "left"],
      ["KeyD", "right"],
      ["Space", "up"],
      ["ShiftLeft", "down"],
      ["ShiftRight", "down"],
      ["ControlLeft", "sprint"],
    ];
    for (const [code, key] of pairs) {
      fireEvent.keyDown(window, { code });
      expect(renderer.setKey).toHaveBeenLastCalledWith(key, true);
      fireEvent.keyUp(window, { code });
      expect(renderer.setKey).toHaveBeenLastCalledWith(key, false);
    }
  });

  it("drops held keys when the window loses focus", () => {
    open();

    fireEvent.blur(window);

    expect(latest().releaseKeys).toHaveBeenCalled();
  });

  it("toggles the performance panel with F3 and shows the frame numbers", () => {
    open();
    act(() =>
      latest().events.onStats(
        {
          refreshHz: 144,
          budgetMs: 6.94,
          intervalP95: 7.1,
          workP95: 2.3,
          missedFrames: 2,
          totalFrames: 1000,
          quality: { renderScale: 0.8, viewFraction: 1 },
        },
        30,
        64,
      ),
    );
    expect(screen.queryByLabelText("性能")).not.toBeInTheDocument();

    fireEvent.keyDown(window, { code: "F3" });

    const panel = screen.getByLabelText("性能");
    expect(within(panel).getByText(/显示器 144 Hz · 每帧预算 6\.9 ms/)).toBeInTheDocument();
    expect(within(panel).getByText(/掉帧 2 \/ 1000/)).toBeInTheDocument();
    expect(within(panel).getByText(/分辨率 80% · 距离 100%/)).toBeInTheDocument();
    expect(within(panel).getByText(/网格 30 \/ 64/)).toBeInTheDocument();
    expect(latest().setShowStats).toHaveBeenLastCalledWith(true);

    fireEvent.keyDown(window, { code: "F3" });
    expect(screen.queryByLabelText("性能")).not.toBeInTheDocument();
    expect(latest().setShowStats).toHaveBeenLastCalledWith(false);
  });

  it("closes with Esc, but Esc only lets go of the mouse while it is captured", () => {
    const { onClose } = open();

    act(() => latest().events.onLockChange(true));
    fireEvent.keyDown(window, { code: "Escape" });
    expect(onClose).not.toHaveBeenCalled();

    act(() => latest().events.onLockChange(false));
    fireEvent.keyDown(window, { code: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("closes from the close button", () => {
    const { onClose } = open();

    fireEvent.click(screen.getByRole("button", { name: "关闭预览" }));

    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("never captures the mouse just because the picture was clicked", () => {
    open();

    fireEvent.click(screen.getByLabelText("3D 画面"));
    fireEvent.mouseDown(screen.getByLabelText("3D 画面"), { clientX: 10, clientY: 10 });
    fireEvent.mouseUp(window);

    expect(latest().requestLock).not.toHaveBeenCalled();
  });

  it("turns the view while a button is held, even when the cursor leaves the picture, and stops on release", () => {
    open();
    const canvas = screen.getByLabelText("3D 画面");

    fireEvent.mouseDown(canvas, { clientX: 100, clientY: 100 });
    expect(canvas).toHaveAttribute("data-dragging", "true");
    fireEvent.mouseMove(window, { clientX: 112, clientY: 95 });
    expect(latest().look).toHaveBeenLastCalledWith(12, -5);
    fireEvent.mouseMove(window, { clientX: 2000, clientY: 95 });
    expect(latest().look).toHaveBeenLastCalledWith(1888, 0);

    fireEvent.mouseUp(window);
    expect(canvas).toHaveAttribute("data-dragging", "false");
    latest().look.mockClear();
    fireEvent.mouseMove(window, { clientX: 5, clientY: 5 });
    expect(latest().look).not.toHaveBeenCalled();
  });

  it("keeps pointing at blocks with the cursor while looking around", () => {
    open();
    const canvas = screen.getByLabelText("3D 画面");

    fireEvent.mouseMove(canvas, { clientX: 40, clientY: 30 });
    expect(latest().setMouse).toHaveBeenCalledWith([40, 30]);
    fireEvent.mouseLeave(canvas);
    expect(latest().setMouse).toHaveBeenLastCalledWith(null);
  });

  it("does not open the browser's context menu on the picture", () => {
    open();
    const prevented = !fireEvent.contextMenu(screen.getByLabelText("3D 画面"));
    expect(prevented).toBe(true);
  });

  it("offers mouse capture as a button, which says what the system hint is, and lets go again", () => {
    open();
    const capture = screen.getByRole("button", { name: "抓取鼠标" });
    expect(capture).toHaveAttribute("title", expect.stringContaining("按 Esc 显示光标"));

    fireEvent.click(capture);
    expect(latest().requestLock).toHaveBeenCalledTimes(1);

    act(() => latest().events.onLockChange(true));
    // While captured, dragging does not also turn the view a second way.
    fireEvent.mouseDown(screen.getByLabelText("3D 画面"), { clientX: 1, clientY: 1 });
    fireEvent.mouseMove(window, { clientX: 50, clientY: 50 });
    expect(latest().look).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "放开鼠标" }));
    expect(latest().releaseLock).toHaveBeenCalledTimes(1);
  });

  it("changes speed with the wheel", () => {
    open();
    const canvas = screen.getByLabelText("3D 画面");

    fireEvent.wheel(canvas, { deltaY: -100 });
    expect(latest().scaleSpeed).toHaveBeenLastCalledWith(expect.closeTo(1.15, 5));
    fireEvent.wheel(canvas, { deltaY: 100 });
    expect(latest().scaleSpeed.mock.lastCall?.[0]).toBeLessThan(1);
  });

  it("changes the help text once the mouse is captured", () => {
    open();
    expect(screen.getByText(/按住鼠标拖动转向/)).toBeInTheDocument();

    act(() => latest().events.onLockChange(true));

    expect(screen.getByText(/Esc 放开鼠标/)).toBeInTheDocument();
  });

  it("cleans up: the renderer is disposed, keys stop working and a drag in progress ends", () => {
    const { unmount } = open();
    const renderer = latest();
    fireEvent.mouseDown(screen.getByLabelText("3D 画面"), { clientX: 1, clientY: 1 });

    unmount();
    fireEvent.mouseMove(window, { clientX: 90, clientY: 90 });
    expect(renderer.look).not.toHaveBeenCalled();

    expect(renderer.dispose).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(window, { code: "KeyW" });
    expect(renderer.setKey).not.toHaveBeenCalled();
  });

  it("says so when the machine has no WebGL", async () => {
    fake.FakeRenderer.failToStart = true;

    open();

    expect(await screen.findByRole("alert")).toHaveTextContent("不支持 WebGL");
  });

  it("says so when the block data does not fit the size", () => {
    render(<SchematicViewer view={{ ...view, runs: toBase64([[1, 2]]), runCount: 1 }} onClose={vi.fn()} />);

    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(fake.FakeRenderer.instances).toHaveLength(0);
  });
});
