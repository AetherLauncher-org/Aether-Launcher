import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useQuery } from "./query";

describe("useQuery", () => {
  it("starts loading and resolves to the loaded value", async () => {
    const load = vi.fn().mockResolvedValue([1, 2, 3]);

    const { result } = renderHook(() => useQuery(load));

    expect(result.current.status).toBe("loading");
    await waitFor(() => expect(result.current.status).toBe("success"));
    expect(result.current.data).toEqual([1, 2, 3]);
    expect(result.current.error).toBeNull();
  });

  it("reports a rejection as an error message", async () => {
    const load = vi.fn().mockRejectedValue("游戏目录不可读");

    const { result } = renderHook(() => useQuery(load));

    await waitFor(() => expect(result.current.status).toBe("error"));
    expect(result.current.error).toBe("游戏目录不可读");
    expect(result.current.data).toBeNull();
  });

  it("turns a rejected Error into its message", async () => {
    const load = vi.fn().mockRejectedValue(new Error("store unavailable"));

    const { result } = renderHook(() => useQuery(load));

    await waitFor(() => expect(result.current.error).toBe("store unavailable"));
  });

  it("does nothing while disabled", async () => {
    const load = vi.fn().mockResolvedValue([]);

    const { result } = renderHook(() => useQuery(load, false));

    expect(result.current.status).toBe("idle");
    expect(load).not.toHaveBeenCalled();
  });

  it("reloads on demand", async () => {
    const load = vi
      .fn()
      .mockResolvedValueOnce(["first"])
      .mockResolvedValueOnce(["second"]);

    const { result } = renderHook(() => useQuery(load));
    await waitFor(() => expect(result.current.data).toEqual(["first"]));

    act(() => {
      result.current.reload();
    });

    await waitFor(() => expect(result.current.data).toEqual(["second"]));
    expect(load).toHaveBeenCalledTimes(2);
  });

  it("keeps the previous data while retrying", async () => {
    let reject = true;
    const load = vi.fn().mockImplementation(() => {
      if (reject) {
        reject = false;
        return Promise.reject("offline");
      }
      return Promise.resolve(["ok"]);
    });

    const { result } = renderHook(() => useQuery(load));
    await waitFor(() => expect(result.current.status).toBe("error"));

    act(() => {
      result.current.reload();
    });
    await waitFor(() => expect(result.current.status).toBe("success"));
    expect(result.current.data).toEqual(["ok"]);
  });
});
