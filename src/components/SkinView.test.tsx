import { fireEvent, render } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { Mat3 } from "../lib/skinModel";
import { SkinView } from "./SkinView";

// jsdom has no 2D canvas, so the real renderer would draw nothing; replace the two calls that
// need one and record what the component asks for.
const rendered: Mat3[] = [];
vi.mock("../lib/skinRender", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/skinRender")>()),
  prepareSkin: () => ({ quads: [] }),
  renderSkin: (_target: HTMLCanvasElement, _skin: unknown, view: Mat3) => {
    rendered.push(view);
    return true;
  },
}));

const image = { width: 64, height: 64 } as unknown as HTMLImageElement;
const last = () => rendered[rendered.length - 1];
const same = (a: Mat3, b: Mat3) => a.every((value, index) => Math.abs(value - b[index]) < 1e-9);

// jsdom has no PointerEvent, and without one the coordinates of an event never arrive. A mouse
// event with a pointer id is what the component reads.
beforeAll(() => {
  class PointerEventStub extends MouseEvent {
    pointerId: number;
    constructor(type: string, init: MouseEventInit & { pointerId?: number } = {}) {
      super(type, init);
      this.pointerId = init.pointerId ?? 1;
    }
  }
  Object.defineProperty(window, "PointerEvent", { value: PointerEventStub, configurable: true });
});

function pointer(
  target: Element,
  type: "pointerDown" | "pointerMove" | "pointerUp",
  init: { clientX?: number; clientY?: number; shiftKey?: boolean; button?: number },
) {
  fireEvent[type](target, { button: 0, pointerId: 1, ...init });
}

describe("SkinView", () => {
  beforeEach(() => {
    rendered.length = 0;
  });

  it("draws once in the default pose and does nothing when the mouse only passes over it", () => {
    const { container } = render(<SkinView image={image} slim={false} />);
    const canvas = container.querySelector("canvas")!;
    const first = last();

    pointer(canvas, "pointerMove", { clientX: 40, clientY: 30 });
    pointer(canvas, "pointerMove", { clientX: 90, clientY: 80 });

    expect(rendered).toHaveLength(1);
    expect(last()).toBe(first);
  });

  it("turns while the button is held, and stops when it is let go", () => {
    const { container } = render(<SkinView image={image} slim={false} />);
    const canvas = container.querySelector("canvas")!;
    const start = last();

    pointer(canvas, "pointerDown", { clientX: 50, clientY: 50 });
    pointer(canvas, "pointerMove", { clientX: 90, clientY: 50 });
    const turned = last();
    expect(same(turned, start)).toBe(false);

    pointer(canvas, "pointerUp", { clientX: 90, clientY: 50 });
    const afterRelease = rendered.length;
    pointer(canvas, "pointerMove", { clientX: 200, clientY: 200 });
    expect(rendered).toHaveLength(afterRelease);
  });

  it("turns about the vertical axis for a sideways drag and tips for an up-down drag", () => {
    const { container } = render(<SkinView image={image} slim={false} />);
    const canvas = container.querySelector("canvas")!;

    pointer(canvas, "pointerDown", { clientX: 50, clientY: 50 });
    pointer(canvas, "pointerMove", { clientX: 50, clientY: 90 });
    const tipped = last();
    pointer(canvas, "pointerUp", {});

    pointer(canvas, "pointerDown", { clientX: 50, clientY: 50 });
    pointer(canvas, "pointerMove", { clientX: 90, clientY: 50 });
    const turned = last();

    expect(same(tipped, turned)).toBe(false);
  });

  it("rolls with Shift held, a third axis the plain drag cannot reach", () => {
    const { container } = render(<SkinView image={image} slim={false} />);
    const canvas = container.querySelector("canvas")!;
    const start = last();

    pointer(canvas, "pointerDown", { clientX: 50, clientY: 50 });
    pointer(canvas, "pointerMove", { clientX: 90, clientY: 50, shiftKey: true });
    const rolled = last();
    pointer(canvas, "pointerUp", {});

    expect(same(rolled, start)).toBe(false);
    // the vertical axis of the figure no longer points straight up on the screen
    expect(Math.abs(rolled[3])).toBeGreaterThan(Math.abs(start[3]));
  });

  it("goes back to the default pose on a double click", () => {
    const { container } = render(<SkinView image={image} slim={false} />);
    const canvas = container.querySelector("canvas")!;
    const start = last();

    pointer(canvas, "pointerDown", { clientX: 50, clientY: 50 });
    pointer(canvas, "pointerMove", { clientX: 120, clientY: 90 });
    pointer(canvas, "pointerUp", {});
    expect(same(last(), start)).toBe(false);

    fireEvent.doubleClick(canvas);

    expect(same(last(), start)).toBe(true);
  });

  it("ignores the right button", () => {
    const { container } = render(<SkinView image={image} slim={false} />);
    const canvas = container.querySelector("canvas")!;

    pointer(canvas, "pointerDown", { clientX: 50, clientY: 50, button: 2 });
    pointer(canvas, "pointerMove", { clientX: 120, clientY: 90 });

    expect(rendered).toHaveLength(1);
  });

  it("is decoration without a label and a picture with one", () => {
    const decorative = render(<SkinView image={image} slim={false} />).container.querySelector("canvas")!;
    expect(decorative).toHaveAttribute("aria-hidden", "true");
    expect(decorative).not.toHaveAttribute("role");

    const labelled = render(<SkinView image={image} slim={false} label="Notch 的全身皮肤预览" />).container.querySelector("canvas")!;
    expect(labelled).toHaveAttribute("role", "img");
    expect(labelled).toHaveAttribute("aria-label", "Notch 的全身皮肤预览");
  });

  it("draws nothing while the picture has not loaded", () => {
    render(<SkinView image={null} slim={false} />);

    expect(rendered).toHaveLength(0);
  });
});
