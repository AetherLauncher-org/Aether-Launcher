import { FluentProvider, webLightTheme } from "@fluentui/react-components";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
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

  it("has one thumb while the allocation fits in the free memory", () => {
    const { container } = renderBar({ valueMiB: 2048 });

    expect(container.querySelectorAll(".memory-thumb")).toHaveLength(1);
    expect(container.querySelector(".memory-seg.used")).not.toBeNull();
    expect(container.querySelector(".memory-seg.over")).toBeNull();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.getByText(/红色是系统已经占用的内存，不能拖/)).toBeInTheDocument();
  });

  it("grows a second thumb at the divider once all the free memory is allocated", () => {
    const { container } = renderBar({ valueMiB: 4096 });

    expect(container.querySelectorAll(".memory-thumb")).toHaveLength(2);
    expect(container.querySelector(".memory-thumb.left")).not.toBeNull();
    // 刚好满，还没有越界
    expect(container.querySelector(".memory-seg.over")).toBeNull();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("paints the part past the free memory and warns about running out", () => {
    const { container } = renderBar({ valueMiB: 8192 });

    expect(container.querySelector(".memory-seg.over")).not.toBeNull();
    expect(screen.getByRole("status")).toHaveTextContent("超过了当前可用内存");
    expect(screen.getByRole("list", { name: "图例" })).toHaveTextContent("超出可用内存");
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
    expect(screen.queryByText(/红色是系统已经占用的内存/)).not.toBeInTheDocument();
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

  it("does not move for a press on the red area before the allocation is full", () => {
    const { onChange, track } = renderBar({ valueMiB: 1024 });

    pointer(track, "pointerdown", 400);

    expect(onChange).not.toHaveBeenCalled();
  });

  it("drags the second thumb into the red area to allocate past the free memory", () => {
    const { onChange, track } = renderBar({ valueMiB: 4096 });

    pointer(track, "pointerdown", 500);
    expect(onChange).toHaveBeenLastCalledWith(8192);

    pointer(track, "pointermove", 0);
    expect(onChange).toHaveBeenLastCalledWith(16_384);
  });

  it("brings the allocation back to full when the second thumb is dragged back to the divider", () => {
    const { onChange, track } = renderBar({ valueMiB: 8192 });

    pointer(track, "pointerdown", 500);
    pointer(track, "pointermove", 800);

    expect(onChange).toHaveBeenLastCalledWith(4096);
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
