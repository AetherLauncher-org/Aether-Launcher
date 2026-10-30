import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { RefreshButton } from "./RefreshButton";

const icon = () => document.querySelector("svg");

describe("RefreshButton", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("sits still until it is used", () => {
    render(<RefreshButton label="刷新列表" loading={false} onClick={() => undefined} />);

    expect(icon()).not.toHaveClass("spin-icon");
    expect(screen.getByRole("button", { name: "刷新列表" })).toBeEnabled();
  });

  it("spins for a full turn after a click even when the reload is instant", () => {
    const onClick = vi.fn();
    render(<RefreshButton label="刷新列表" loading={false} onClick={onClick} />);

    fireEvent.click(screen.getByRole("button", { name: "刷新列表" }));

    expect(onClick).toHaveBeenCalledTimes(1);
    expect(icon()).toHaveClass("spin-icon");

    act(() => {
      vi.advanceTimersByTime(700);
    });
    expect(icon()).not.toHaveClass("spin-icon");
  });

  it("keeps spinning, and cannot be clicked again, while it is loading", () => {
    const onClick = vi.fn();
    render(<RefreshButton label="刷新列表" loading onClick={onClick} />);

    const button = screen.getByRole("button", { name: "刷新列表" });
    expect(icon()).toHaveClass("spin-icon");
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("aria-busy", "true");
    fireEvent.click(button);
    expect(onClick).not.toHaveBeenCalled();
  });
});
