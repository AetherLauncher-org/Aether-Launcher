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
function pointer(target: Element, type: string, clientX: number) {
  act(() => {
    target.dispatchEvent(new MouseEvent(type, { bubbles: true, clientX }));
  });
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
    expect(screen.getByText(/往左拖橙色手柄可以多分配/)).toBeInTheDocument();
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
    expect(screen.queryByText(/往左拖橙色手柄可以多分配/)).not.toBeInTheDocument();
  });

  it("says so when the machine's memory could not be read", () => {
    renderBar({ system: null, valueMiB: 4096 });

    expect(screen.getByText(/没能读取本机内存，范围按 16 GiB 估计/)).toBeInTheDocument();
  });
});

describe("MemoryBar dragging", () => {
  it("moves the right thumb inside the free area and snaps to the step", () => {
    const { onChange, track } = renderBar({ valueMiB: 1024 });

    // 90% 处 = 分界线右边 15% = 2457.6 MiB，取整到 2560
    pointer(track, "pointerdown", 900);

    expect(onChange).toHaveBeenLastCalledWith(2560);
  });

  it("follows the pointer while it is held and stops when it is released", () => {
    const { onChange, track } = renderBar({ valueMiB: 1024 });

    pointer(track, "pointerdown", 800);
    pointer(track, "pointermove", 1000);
    expect(onChange).toHaveBeenLastCalledWith(4096);

    onChange.mockClear();
    pointer(track, "pointerup", 1000);
    pointer(track, "pointermove", 800);
    expect(onChange).not.toHaveBeenCalled();
  });

  it("puts the orange thumb where a press lands in the red area", () => {
    const { onChange, track } = renderBar({ valueMiB: 1024 });

    pointer(track, "pointerdown", 500);

    expect(onChange).toHaveBeenLastCalledWith(8192);
  });

  it("grabbing the orange thumb changes nothing until it is dragged", () => {
    const { onChange, container } = renderBar({ valueMiB: 1024 });
    const orange = container.querySelector(".memory-thumb.left") as HTMLElement;

    pointer(orange, "pointerdown", 750);

    expect(onChange).not.toHaveBeenCalled();
  });

  it("dragging the orange thumb left grows the blue share while it has room, instead of moving orange", () => {
    const { onChange, container } = renderBar({ valueMiB: 1024 });
    const orange = container.querySelector(".memory-thumb.left") as HTMLElement;

    pointer(orange, "pointerdown", 750);
    // 向左 50px = 5% = 819 MiB，取整到 1792
    pointer(orange, "pointermove", 700);

    expect(onChange).toHaveBeenLastCalledWith(1792);
    // 这个值还在可用内存之内：橙区不会出现，橙色手柄还在分界线上
    expect(barGeometryOver(1792)).toBe(0);
  });

  it("only moves the orange thumb into the red once the blue share has reached the end", () => {
    const { onChange, container } = renderBar({ valueMiB: 1024 });
    const orange = container.querySelector(".memory-thumb.left") as HTMLElement;

    pointer(orange, "pointerdown", 750);
    pointer(orange, "pointermove", 500);

    // 向左 25% = 4096 MiB：先把蓝色加到「满」(4096)，剩下的才越过分界线
    expect(onChange).toHaveBeenLastCalledWith(1024 + 4096);
    expect(barGeometryOver(1024 + 4096)).toBe(1024);
  });

  it("dragging the orange thumb right first brings it back to the divider, then shrinks the blue share", () => {
    const { onChange, container } = renderBar({ valueMiB: 8192 });
    const orange = container.querySelector(".memory-thumb.left") as HTMLElement;

    pointer(orange, "pointerdown", 500);
    pointer(orange, "pointermove", 750);
    expect(onChange).toHaveBeenLastCalledWith(4096);

    pointer(orange, "pointermove", 800);
    const shrunk = onChange.mock.calls[onChange.mock.calls.length - 1][0] as number;
    expect(shrunk).toBeLessThan(4096);
  });

  it("stops following the pointer once it is released", () => {
    const { onChange, container } = renderBar({ valueMiB: 1024 });
    const orange = container.querySelector(".memory-thumb.left") as HTMLElement;

    pointer(orange, "pointerdown", 750);
    pointer(orange, "pointerup", 750);
    pointer(orange, "pointermove", 500);

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
