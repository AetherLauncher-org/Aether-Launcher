import { FluentProvider, webLightTheme } from "@fluentui/react-components";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { barGeometry } from "../lib/memoryBar";
import { MemoryBar } from "./MemoryBar";

// 16 GiB，已用 12 GiB，可用 4 GiB：分界线在轨道的 75% 处，「满」是 4096 MiB
const machine = { totalMib: 16_384, availableMib: 4096 };

function renderBar(props: Partial<Parameters<typeof MemoryBar>[0]> = {}) {
  const onChange = vi.fn();
  const view = render(
    <FluentProvider theme={webLightTheme}>
      <MemoryBar system={machine} valueMiB={1024} onChange={onChange} {...props} />
    </FluentProvider>,
  );
  const track = view.container.querySelector(".memory-track") as HTMLElement;
  // jsdom 没有布局：让轨道看起来是 1000px 宽，这样 clientX 就等于百分比的 10 倍
  vi.spyOn(track, "getBoundingClientRect").mockReturnValue({
    left: 0,
    top: 0,
    right: 1000,
    bottom: 22,
    width: 1000,
    height: 22,
    x: 0,
    y: 0,
    toJSON: () => ({}),
  });
  return { onChange, track, ...view };
}

/** 某个分配值在这台机器上超出可用内存多少。 */
function barGeometryOver(valueMiB: number): number {
  return barGeometry(valueMiB, machine).overMiB;
}

/** jsdom 的指针事件不带坐标，用带 clientX 的鼠标事件冒充同名事件。 */
function pointer(target: Element, type: string, clientX: number, shiftKey = false): MouseEvent {
  const event = new MouseEvent(type, { bubbles: true, cancelable: true, clientX, shiftKey });
  act(() => {
    target.dispatchEvent(event);
  });
  return event;
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("MemoryBar layout", () => {
  it("shows the system's use, the game's share and what is free", () => {
    renderBar({ valueMiB: 2048 });

    expect(screen.getByText("12 GiB / 16 GiB")).toBeInTheDocument();
    expect(screen.getByText("2 GiB（可用 4 GiB）")).toBeInTheDocument();
    expect(screen.getByRole("list", { name: "图例" })).toHaveTextContent("游戏分配");
    expect(screen.getByRole("list", { name: "图例" })).toHaveTextContent("系统已用");
  });

  it("always shows both thumbs, with the orange one on the divider while the allocation fits", () => {
    const { container } = renderBar({ valueMiB: 2048 });

    expect(container.querySelectorAll(".memory-thumb")).toHaveLength(2);
    expect(container.querySelector(".memory-thumb.left")).not.toBeNull();
    expect(container.querySelector(".memory-seg.used")).not.toBeNull();
    expect(container.querySelector(".memory-seg.over")).toBeNull();
    expect(container.querySelector(".memory-over-label")).toBeNull();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.getByText(/都是多分配/)).toBeInTheDocument();
  });

  it("has no orange zone yet when the free memory is exactly allocated", () => {
    const { container } = renderBar({ valueMiB: 4096 });

    expect(container.querySelectorAll(".memory-thumb")).toHaveLength(2);
    expect(container.querySelector(".memory-seg.over")).toBeNull();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("calls the orange zone not recommended, in the bar, the legend and the warning", () => {
    const { container } = renderBar({ valueMiB: 8192 });

    expect(container.querySelector(".memory-seg.over")).not.toBeNull();
    expect(container.querySelector(".memory-over-label")).toHaveTextContent("不推荐");
    expect(screen.getByRole("list", { name: "图例" })).toHaveTextContent("超出可用内存（不推荐）");
    expect(screen.getByRole("status")).toHaveTextContent("不推荐：分配的内存超过了当前可用内存");
    expect(screen.getByText("8 GiB（可用 4 GiB）")).toBeInTheDocument();
  });

  it("drops the red area, the divider and the second thumb when the used memory is unknown", () => {
    const { container } = renderBar({
      system: { totalMib: 8192, availableMib: null },
      valueMiB: 8192,
    });

    expect(screen.getByText("— / 8 GiB")).toBeInTheDocument();
    expect(container.querySelector(".memory-seg.used")).toBeNull();
    expect(container.querySelector(".memory-divider")).toBeNull();
    expect(container.querySelectorAll(".memory-thumb")).toHaveLength(1);
    expect(screen.queryByText(/都是多分配/)).not.toBeInTheDocument();
  });

  it("says so when the machine's memory could not be read", () => {
    renderBar({ system: null, valueMiB: 4096 });

    expect(screen.getByText(/没能读取本机内存，范围按 16 GiB 估计/)).toBeInTheDocument();
  });
});

describe("MemoryBar dragging", () => {
  const blue = (container: HTMLElement) => container.querySelector(".memory-thumb.right") as HTMLElement;
  const orange = (container: HTMLElement) => container.querySelector(".memory-thumb.left") as HTMLElement;

  it("jumps the blue thumb to a press in the free area, snapped to 512 MiB", () => {
    const { onChange, track } = renderBar({ valueMiB: 1024 });

    // 90% 处 = 分界线右边 15% = 2457.6 MiB → 2560
    pointer(track, "pointerdown", 900);

    expect(onChange).toHaveBeenLastCalledWith(2560);
  });

  it("does not snap a press made with Shift held, in steps fine enough to look continuous", () => {
    const { onChange, track } = renderBar({ valueMiB: 1024 });

    pointer(track, "pointerdown", 900, true);

    expect(onChange).toHaveBeenLastCalledWith(2464);
  });

  it("snaps every move to a multiple of 512 MiB, until Shift is held", () => {
    const { onChange, container } = renderBar({ valueMiB: 1024 });

    pointer(blue(container), "pointerdown", 812);
    for (const x of [822, 835, 851, 870, 893]) {
      pointer(blue(container), "pointermove", x);
      expect((onChange.mock.calls[onChange.mock.calls.length - 1][0] as number) % 512).toBe(0);
    }

    pointer(blue(container), "pointermove", 893, true);
    expect((onChange.mock.calls[onChange.mock.calls.length - 1][0] as number) % 512).not.toBe(0);
  });

  it("lets Shift be pressed and released in the middle of one drag", () => {
    const { onChange, container } = renderBar({ valueMiB: 1024 });

    pointer(blue(container), "pointerdown", 812);
    pointer(blue(container), "pointermove", 862, true); // +5% = 819 → 1843.2 → 1856（细）
    expect(onChange).toHaveBeenLastCalledWith(1856);
    pointer(blue(container), "pointermove", 862); // 松开 Shift：回到网格
    expect(onChange).toHaveBeenLastCalledWith(2048);
  });

  it("grabbing either thumb changes nothing until it moves", () => {
    const { onChange, container } = renderBar({ valueMiB: 1024 });

    pointer(blue(container), "pointerdown", 812);
    pointer(orange(container), "pointerdown", 750);

    expect(onChange).not.toHaveBeenCalled();
  });

  it("keeps the thumb under the pointer: the blue thumb moves by how far the pointer moved, not to where it is", () => {
    const { onChange, container } = renderBar({ valueMiB: 1024 });

    // 手柄在 81.25%，抓在它偏右一点的 830 处；向右拖 50px = 5% = 819 MiB
    pointer(blue(container), "pointerdown", 830);
    pointer(blue(container), "pointermove", 880, true);

    expect(onChange).toHaveBeenLastCalledWith(1856);
  });

  it("dragging the blue thumb back left allocates less, down to the minimum", () => {
    const { onChange, container } = renderBar({ valueMiB: 1024 });

    pointer(blue(container), "pointerdown", 812);
    pointer(blue(container), "pointermove", 792, true); // 左移 2% = 328 MiB
    expect(onChange).toHaveBeenLastCalledWith(704);

    pointer(blue(container), "pointermove", 700);
    expect(onChange).toHaveBeenLastCalledWith(512);
  });

  it("pushing the blue thumb past the end carries on into the red, just like dragging the orange thumb", () => {
    const { onChange, container } = renderBar({ valueMiB: 4096 });

    // 蓝色已经在最右边（满）。指针拖到轨道外面也要继续算：再往右 25% = 再多分配 4096 MiB
    pointer(blue(container), "pointerdown", 1000);
    pointer(blue(container), "pointermove", 1250);

    expect(onChange).toHaveBeenLastCalledWith(8192);
    expect(barGeometryOver(8192)).toBe(4096);
  });

  it("dragging the blue thumb back left first pulls the orange thumb to the divider, then shrinks blue", () => {
    const { onChange, container } = renderBar({ valueMiB: 8192 });

    pointer(blue(container), "pointerdown", 1000);
    pointer(blue(container), "pointermove", 750);
    expect(onChange).toHaveBeenLastCalledWith(4096);
    expect(barGeometryOver(4096)).toBe(0);

    pointer(blue(container), "pointermove", 700);
    const shrunk = onChange.mock.calls[onChange.mock.calls.length - 1][0] as number;
    expect(shrunk).toBeLessThan(4096);
  });

  it("follows the pointer while it is held and stops when it is released", () => {
    const { onChange, track } = renderBar({ valueMiB: 1024 });

    pointer(track, "pointerdown", 800);
    pointer(track, "pointermove", 1000);
    expect(onChange).toHaveBeenCalledTimes(2);

    onChange.mockClear();
    pointer(track, "pointerup", 1000);
    pointer(track, "pointermove", 800);
    expect(onChange).not.toHaveBeenCalled();
  });

  it("puts the orange thumb where a press lands in the red area, and carries on from there", () => {
    const { onChange, track } = renderBar({ valueMiB: 1024 });

    pointer(track, "pointerdown", 500);
    expect(onChange).toHaveBeenLastCalledWith(8192);

    // 从按下的地方继续：再往左 10% = 再多分配 1638 MiB
    pointer(track, "pointermove", 400, true);
    expect(onChange).toHaveBeenLastCalledWith(8192 + 1632);
  });

  it("dragging the orange thumb left grows the blue share while it has room, instead of moving orange", () => {
    const { onChange, container } = renderBar({ valueMiB: 1024 });

    pointer(orange(container), "pointerdown", 750);
    // 向左 50px = 5% = 819 MiB，按住 Shift 时指针粒度 32 MiB
    pointer(orange(container), "pointermove", 700, true);

    expect(onChange).toHaveBeenLastCalledWith(1856);
    expect(barGeometryOver(1856)).toBe(0);
  });

  it("only moves the orange thumb into the red once the blue share has reached the end", () => {
    const { onChange, container } = renderBar({ valueMiB: 1024 });

    pointer(orange(container), "pointerdown", 750);
    pointer(orange(container), "pointermove", 500);

    expect(onChange).toHaveBeenLastCalledWith(1024 + 4096);
    expect(barGeometryOver(1024 + 4096)).toBe(1024);
  });

  it("dragging the orange thumb right first brings it back to the divider, then shrinks the blue share", () => {
    const { onChange, container } = renderBar({ valueMiB: 8192 });

    pointer(orange(container), "pointerdown", 500);
    pointer(orange(container), "pointermove", 750);
    expect(onChange).toHaveBeenLastCalledWith(4096);

    pointer(orange(container), "pointermove", 800);
    const shrunk = onChange.mock.calls[onChange.mock.calls.length - 1][0] as number;
    expect(shrunk).toBeLessThan(4096);
  });

  it("gives both thumbs the same result for the same effort in the 'more' direction", () => {
    const viaBlue = renderBar({ valueMiB: 1024 });
    pointer(blue(viaBlue.container), "pointerdown", 812);
    pointer(blue(viaBlue.container), "pointermove", 862);
    const blueResult = viaBlue.onChange.mock.calls[viaBlue.onChange.mock.calls.length - 1][0];
    viaBlue.unmount();

    const viaOrange = renderBar({ valueMiB: 1024 });
    pointer(orange(viaOrange.container), "pointerdown", 750);
    pointer(orange(viaOrange.container), "pointermove", 700);
    const orangeResult = viaOrange.onChange.mock.calls[viaOrange.onChange.mock.calls.length - 1][0];

    expect(blueResult).toBe(orangeResult);
  });

  it("stops following the pointer once it is released", () => {
    const { onChange, container } = renderBar({ valueMiB: 1024 });

    pointer(orange(container), "pointerdown", 750);
    pointer(orange(container), "pointerup", 750);
    pointer(orange(container), "pointermove", 500);

    expect(onChange).not.toHaveBeenCalled();
  });

  it("draws the snapping ticks on the track", () => {
    const { container } = renderBar({ valueMiB: 1024 });

    expect(container.querySelectorAll(".memory-tick").length).toBeGreaterThan(10);
    expect(container.querySelectorAll(".memory-tick.major").length).toBeGreaterThan(0);
  });

  it("shows the focus ring only for real keyboard use, not for a press or the Shift key", () => {
    const { container, track } = renderBar({ valueMiB: 1024 });
    const bar = container.querySelector(".memory-bar") as HTMLElement;
    const input = screen.getByRole("slider", { name: "游戏内存上限" });

    pointer(track, "pointerdown", 900);
    fireEvent.keyDown(input, { key: "Shift" });
    expect(bar).toHaveAttribute("data-keyboard", "false");

    fireEvent.keyDown(input, { key: "ArrowRight" });
    expect(bar).toHaveAttribute("data-keyboard", "true");

    pointer(track, "pointerdown", 900);
    expect(bar).toHaveAttribute("data-keyboard", "false");
  });

  it("takes over the press so the browser never starts a text selection or native drag", () => {
    const { track, container } = renderBar({ valueMiB: 1024 });

    // 否则指针会变成「禁止」图标，后面的移动也收不到
    expect(pointer(track, "pointerdown", 900).defaultPrevented).toBe(true);
    expect(pointer(container.querySelector(".memory-thumb.left") as Element, "pointerdown", 750).defaultPrevented).toBe(true);
    const dragStart = new Event("dragstart", { bubbles: true, cancelable: true });
    act(() => {
      track.dispatchEvent(dragStart);
    });
    expect(dragStart.defaultPrevented).toBe(true);
  });

  it("leaves the press alone when the bar is read-only", () => {
    const { track } = renderBar({ disabled: true });

    expect(pointer(track, "pointerdown", 900).defaultPrevented).toBe(false);
  });

  it("stops following the pointer when the browser takes the pointer away", () => {
    const { onChange, track } = renderBar({ valueMiB: 1024 });

    pointer(track, "pointerdown", 800);
    onChange.mockClear();
    act(() => {
      track.dispatchEvent(new Event("lostpointercapture", { bubbles: true }));
    });
    pointer(track, "pointermove", 950);

    expect(onChange).not.toHaveBeenCalled();
  });

  it("ignores the pointer when read-only", () => {
    const { onChange, track } = renderBar({ onChange: undefined });

    pointer(track, "pointerdown", 900);
    pointer(track, "pointermove", 950);

    expect(onChange).not.toHaveBeenCalled();
    expect(screen.getByRole("slider", { name: "游戏内存上限" })).toBeDisabled();
  });

  it("ignores the pointer when disabled", () => {
    const { onChange, track } = renderBar({ disabled: true });

    pointer(track, "pointerdown", 900);

    expect(onChange).not.toHaveBeenCalled();
  });
});

describe("MemoryBar keyboard and screen readers", () => {
  it("exposes a real slider that stays in step with the thumbs", () => {
    renderBar({ valueMiB: 6144 });

    const slider = screen.getByRole("slider", { name: "游戏内存上限" });
    expect(slider).toHaveValue("6144");
    expect(slider).toHaveAttribute("min", "512");
    expect(slider).toHaveAttribute("max", "16384");
    expect(slider).toHaveAttribute("step", "256");
    expect(slider).toHaveAttribute("aria-valuetext", "6 GiB");
  });

  it("reports changes made through the slider, for keyboards and assistive tech", () => {
    const { onChange } = renderBar();

    fireEvent.change(screen.getByRole("slider", { name: "游戏内存上限" }), { target: { value: "8192" } });

    expect(onChange).toHaveBeenCalledWith(8192);
  });

  it("keeps the thumbs and the label inside the range for an out-of-range value", () => {
    renderBar({ valueMiB: 999_999 });

    expect(screen.getByRole("slider", { name: "游戏内存上限" })).toHaveValue("16384");
  });
});
