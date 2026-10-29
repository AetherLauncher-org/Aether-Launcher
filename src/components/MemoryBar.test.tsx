import { FluentProvider, webLightTheme } from "@fluentui/react-components";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { barGeometry } from "../lib/memoryBar";
import { MemoryBar } from "./MemoryBar";

// 16 GiB，已用 12 GiB，还能分 4 GiB：红区从右端往左铺，左边缘在 25%
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

/** jsdom 的指针事件不带坐标，用带 clientX 的鼠标事件冒充同名事件。 */
function pointer(target: Element, type: string, clientX: number, shiftKey = false): MouseEvent {
  const event = new MouseEvent(type, { bubbles: true, cancelable: true, clientX, shiftKey });
  act(() => {
    target.dispatchEvent(event);
  });
  return event;
}

/** 唯一的手柄。 */
const thumb = (container: HTMLElement) => container.querySelector(".memory-thumb") as HTMLElement;

function lastValue(mock: ReturnType<typeof vi.fn>): number {
  return mock.mock.calls[mock.mock.calls.length - 1][0] as number;
}

/** 轨道上的 CSS 变量，用来确认两段色的位置。 */
function cssVar(container: HTMLElement, name: string): number {
  const bar = container.querySelector(".memory-bar") as HTMLElement;
  return Number.parseFloat(bar.style.getPropertyValue(name));
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

  it("draws one thumb, so there is only ever one direction to drag", () => {
    const { container } = renderBar({ valueMiB: 2048 });

    expect(container.querySelectorAll(".memory-thumb")).toHaveLength(1);
    expect(container.querySelector(".memory-seg.used")).not.toBeNull();
    expect(container.querySelector(".memory-seg.alloc")).not.toBeNull();
  });

  it("grows the red zone in from the right end, not from the left", () => {
    const { container } = renderBar({ valueMiB: 2048 });

    // 蓝段从头（左）开始；红段从 25% 起一直铺到右端
    expect(cssVar(container, "--pos")).toBeCloseTo(12.5);
    expect(cssVar(container, "--used-start")).toBeCloseTo(25);
  });

  it("anchors the red zone to the right whatever the allocation is", () => {
    for (const valueMiB of [512, 4096, 8192, 16_384]) {
      const { container, unmount } = renderBar({ valueMiB });
      expect(cssVar(container, "--used-start")).toBeCloseTo(25);
      unmount();
    }
  });

  it("has no warning while the allocation still fits", () => {
    const { container } = renderBar({ valueMiB: 4096 });

    expect(container.querySelector(".memory-bar")).toHaveAttribute("data-over", "false");
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("warns, in the bar and in the text, once the allocation passes the free memory", () => {
    const { container } = renderBar({ valueMiB: 8192 });

    expect(container.querySelector(".memory-bar")).toHaveAttribute("data-over", "true");
    expect(screen.getByRole("status")).toHaveTextContent("不推荐：分配的内存超过了当前可用内存");
    expect(screen.getByText("8 GiB（可用 4 GiB）")).toBeInTheDocument();
  });

  it("keeps the thumb to the right of a smaller allocation, even past the free memory", () => {
    // 旧版越界之后另一个手柄会往左走，这里钉住「手柄只往右」。
    const small = renderBar({ valueMiB: 1024 });
    const smallPos = cssVar(small.container, "--pos");
    small.unmount();

    const big = renderBar({ valueMiB: 12_288 });
    const bigPos = cssVar(big.container, "--pos");

    expect(bigPos).toBeGreaterThan(smallPos);
  });

  it("drops the red zone when the used memory is unknown", () => {
    const { container } = renderBar({
      system: { totalMib: 8192, availableMib: null },
      valueMiB: 8192,
    });

    expect(screen.getByText(/—\s*\/\s*8 GiB/)).toBeInTheDocument();
    expect(container.querySelector(".memory-seg.used")).toBeNull();
    expect(cssVar(container, "--used-start")).toBe(100);
    expect(container.querySelectorAll(".memory-thumb")).toHaveLength(1);
  });

  it("says so when the machine's memory could not be read", () => {
    renderBar({ system: null, valueMiB: 4096 });

    expect(screen.getByText(/没能读取本机内存，范围按 16 GiB 估计/)).toBeInTheDocument();
  });
});

describe("MemoryBar dragging", () => {
  it("jumps the thumb to a press on the track, snapped to 512 MiB", () => {
    const { onChange, track } = renderBar({ valueMiB: 1024 });

    // 90% × 16 GiB = 14745.6 MiB → 吸附到 14848
    pointer(track, "pointerdown", 900);

    expect(onChange).toHaveBeenLastCalledWith(14848);
  });

  it("does not snap a press made with Shift held, in steps fine enough to look continuous", () => {
    const { onChange, track } = renderBar({ valueMiB: 1024 });

    pointer(track, "pointerdown", 900, true);

    expect(onChange).toHaveBeenLastCalledWith(14752);
  });

  it("snaps every move to a multiple of 512 MiB, until Shift is held", () => {
    const { onChange, container } = renderBar({ valueMiB: 1024 });

    pointer(thumb(container), "pointerdown", 62);
    for (const x of [72, 85, 101, 120, 143]) {
      pointer(thumb(container), "pointermove", x);
      expect(lastValue(onChange) % 512).toBe(0);
    }

    pointer(thumb(container), "pointermove", 143, true);
    expect(lastValue(onChange) % 512).not.toBe(0);
  });

  it("lets Shift be pressed and released in the middle of one drag", () => {
    const { onChange, container } = renderBar({ valueMiB: 1024 });

    pointer(thumb(container), "pointerdown", 62);
    pointer(thumb(container), "pointermove", 112, true); // +5% = 819 → 1843.2 → 1856（细粒度）
    expect(onChange).toHaveBeenLastCalledWith(1856);
    pointer(thumb(container), "pointermove", 112); // 松开 Shift：回到网格
    expect(onChange).toHaveBeenLastCalledWith(2048);
  });

  it("changes nothing when the thumb is grabbed without moving", () => {
    const { onChange, container } = renderBar({ valueMiB: 1024 });

    pointer(thumb(container), "pointerdown", 62);

    expect(onChange).not.toHaveBeenCalled();
  });

  it("moves the thumb by how far the pointer moved, not to where the pointer is", () => {
    const { onChange, container } = renderBar({ valueMiB: 1024 });

    // 手柄在 6.25%（x=62），抓在它偏右一点的 80 处；向右拖 50px = 5% = 819 MiB
    pointer(thumb(container), "pointerdown", 80);
    pointer(thumb(container), "pointermove", 130, true);

    expect(onChange).toHaveBeenLastCalledWith(1856);
  });

  it("allocates less when dragged left, down to the minimum", () => {
    const { onChange, container } = renderBar({ valueMiB: 1024 });

    pointer(thumb(container), "pointerdown", 62);
    pointer(thumb(container), "pointermove", 42, true); // 左移 2% = 328 MiB
    expect(onChange).toHaveBeenLastCalledWith(704);

    pointer(thumb(container), "pointermove", -200);
    expect(onChange).toHaveBeenLastCalledWith(512);
  });

  it("carries on past the free memory in the same direction", () => {
    const { onChange, container } = renderBar({ valueMiB: 4096 });

    // 已经用满可用内存（25%）。继续往右 25% = 再多分配 4096 MiB
    pointer(thumb(container), "pointerdown", 250);
    pointer(thumb(container), "pointermove", 500);

    expect(onChange).toHaveBeenLastCalledWith(8192);
    expect(barGeometry(8192, machine).over).toBe(true);
    // 再往右仍然是加，而不是像以前那样倒回去
    pointer(thumb(container), "pointermove", 750);
    expect(lastValue(onChange)).toBe(12_288);
    expect(lastValue(onChange)).toBeGreaterThan(8192);
  });

  it("comes back out of the warning zone by dragging left, the way it went in", () => {
    const { onChange, container } = renderBar({ valueMiB: 8192 });

    pointer(thumb(container), "pointerdown", 500);
    pointer(thumb(container), "pointermove", 250);
    expect(onChange).toHaveBeenLastCalledWith(4096);
    expect(barGeometry(4096, machine).over).toBe(false);

    pointer(thumb(container), "pointermove", 200);
    expect(lastValue(onChange)).toBeLessThan(4096);
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

  it("stops following the pointer once it is released", () => {
    const { onChange, container } = renderBar({ valueMiB: 1024 });

    pointer(thumb(container), "pointerdown", 500);
    pointer(thumb(container), "pointerup", 500);
    pointer(thumb(container), "pointermove", 100);

    expect(onChange).not.toHaveBeenCalled();
  });

  it("does not owe the overshoot: after pushing past the end, turning around moves the thumb at once", () => {
    const { onChange, container } = renderBar({ valueMiB: 4096 });

    pointer(thumb(container), "pointerdown", 250);
    pointer(thumb(container), "pointermove", 1050); // 早已远超最大值
    expect(onChange).toHaveBeenLastCalledWith(16_384);

    pointer(thumb(container), "pointermove", 1000); // 掉头 5% = 819 MiB
    expect(onChange).toHaveBeenLastCalledWith(16_384 - 1024);
  });

  it("does not owe the overshoot at the minimum either", () => {
    const { onChange, container } = renderBar({ valueMiB: 1024 });

    pointer(thumb(container), "pointerdown", 62);
    pointer(thumb(container), "pointermove", -450);
    expect(onChange).toHaveBeenLastCalledWith(512);

    pointer(thumb(container), "pointermove", -400); // 回头 5%
    expect(onChange).toHaveBeenLastCalledWith(1536);
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
    expect(pointer(thumb(container), "pointerdown", 62).defaultPrevented).toBe(true);
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
  it("exposes a real slider that stays in step with the thumb", () => {
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

  it("keeps the thumb and the label inside the range for an out-of-range value", () => {
    renderBar({ valueMiB: 999_999 });

    expect(screen.getByRole("slider", { name: "游戏内存上限" })).toHaveValue("16384");
  });
});
