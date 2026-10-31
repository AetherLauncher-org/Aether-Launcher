import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { loadSkin } from "../lib/skin";
import { drawHead } from "../lib/skinHead";
import { AccountAvatar } from "./AccountAvatar";

vi.mock("../lib/skin", () => ({ loadSkin: vi.fn() }));
vi.mock("../lib/skinHead", () => ({ drawHead: vi.fn(() => true) }));

const uuid = "069a79f444e94726a5befca90e38aaf5";
const other = "853c80ef3c3749fdaa49938b674adae6";

afterEach(() => {
  vi.resetAllMocks();
  vi.mocked(drawHead).mockReturnValue(true);
});

function avatar(container: HTMLElement) {
  return container.querySelector(".account-avatar");
}

describe("AccountAvatar", () => {
  it("is a letter for an account with no player id, and asks for no skin", () => {
    const { container } = render(<AccountAvatar name="Steve" size={40} />);

    expect(avatar(container)).toHaveTextContent("S");
    expect(container.querySelector("canvas")).toBeNull();
    expect(loadSkin).not.toHaveBeenCalled();
  });

  it("is a neutral person when there is no account", () => {
    const { container } = render(<AccountAvatar name={null} />);

    expect(avatar(container)).toHaveClass("empty");
  });

  it("starts as the letter and becomes the drawn head when the skin arrives", async () => {
    const skin = {} as HTMLImageElement;
    vi.mocked(loadSkin).mockResolvedValue(skin);

    const { container } = render(<AccountAvatar name="Aether_254" uuid={uuid} size={48} />);
    expect(avatar(container)).toHaveTextContent("A");

    await waitFor(() => expect(container.querySelector("canvas.account-avatar.head")).not.toBeNull());
    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    expect(canvas.style.width).toBe("48px");
    expect(canvas.width).toBeGreaterThanOrEqual(96);
    expect(canvas).toHaveAttribute("aria-hidden", "true");
    await waitFor(() => expect(drawHead).toHaveBeenCalledWith(canvas, skin));
    expect(loadSkin).toHaveBeenCalledWith(uuid);
  });

  it("stays the letter when the player has no skin", async () => {
    vi.mocked(loadSkin).mockResolvedValue(null);

    const { container } = render(<AccountAvatar name="Alex" uuid={uuid} />);
    await waitFor(() => expect(loadSkin).toHaveBeenCalled());

    expect(container.querySelector("canvas")).toBeNull();
    expect(avatar(container)).toHaveTextContent("A");
  });

  it("does not show one player's head on another account", async () => {
    vi.mocked(loadSkin).mockImplementation(async (id) => (id === uuid ? ({} as HTMLImageElement) : null));

    const { container, rerender } = render(<AccountAvatar name="First" uuid={uuid} />);
    await waitFor(() => expect(container.querySelector("canvas")).not.toBeNull());

    rerender(<AccountAvatar name="Second" uuid={other} />);

    await waitFor(() => expect(container.querySelector("canvas")).toBeNull());
    expect(screen.queryByRole("img")).toBeNull();
    expect(avatar(container)).toHaveTextContent("S");
  });
});
