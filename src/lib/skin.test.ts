import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getAccountSkin } from "../ipc";
import { forgetSkins, loadSkin } from "./skin";
import { canDrawHead } from "./skinHead";

vi.mock("../ipc", () => ({ getAccountSkin: vi.fn() }));
vi.mock("./skinHead", () => ({ canDrawHead: vi.fn() }));

const uuid = "069a79f444e94726a5befca90e38aaf5";

class FakeImage {
  onload: (() => void) | null = null;
  onerror: (() => void) | null = null;
  set src(value: string) {
    queueMicrotask(() => (value.startsWith("data:image/png") ? this.onload?.() : this.onerror?.()));
  }
}

beforeEach(() => {
  forgetSkins();
  vi.mocked(canDrawHead).mockReturnValue(true);
  vi.stubGlobal("Image", FakeImage);
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.resetAllMocks();
});

describe("loadSkin", () => {
  it("decodes the skin the backend returns", async () => {
    vi.mocked(getAccountSkin).mockResolvedValue("data:image/png;base64,AAAA");

    const image = await loadSkin(uuid);

    expect(image).toBeInstanceOf(FakeImage);
    expect(getAccountSkin).toHaveBeenCalledWith(uuid);
  });

  it("asks only once per player while there is a skin", async () => {
    vi.mocked(getAccountSkin).mockResolvedValue("data:image/png;base64,AAAA");

    const first = await loadSkin(uuid);
    const second = await loadSkin(uuid);

    expect(second).toBe(first);
    expect(getAccountSkin).toHaveBeenCalledTimes(1);
  });

  it("gives null for a player without a skin, and asks again next time", async () => {
    vi.mocked(getAccountSkin).mockResolvedValue(null);

    expect(await loadSkin(uuid)).toBeNull();
    await Promise.resolve();
    expect(await loadSkin(uuid)).toBeNull();

    expect(getAccountSkin).toHaveBeenCalledTimes(2);
  });

  it("gives null when the picture cannot be decoded", async () => {
    vi.mocked(getAccountSkin).mockResolvedValue("data:text/html,nope");

    expect(await loadSkin(uuid)).toBeNull();
  });

  it("gives null when the backend fails", async () => {
    vi.mocked(getAccountSkin).mockRejectedValue(new Error("boom"));

    expect(await loadSkin(uuid)).toBeNull();
  });

  it("does not even ask where no head can be drawn", async () => {
    vi.mocked(canDrawHead).mockReturnValue(false);

    expect(await loadSkin(uuid)).toBeNull();
    expect(getAccountSkin).not.toHaveBeenCalled();
  });
});
