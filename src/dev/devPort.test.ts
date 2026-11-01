import { describe, expect, it } from "vitest";
import { defaultPort, findPort } from "../../scripts/dev-port.mjs";

// `canBind` / `isFree` talk to the real network stack; they are checked by hand with
// `node scripts/tauri-dev.mjs --dry-run`. The search itself is pure once `free` is injected.
describe("dev port picker", () => {
  it("keeps the default port when it can be bound", async () => {
    expect(await findPort(defaultPort, 1000, async () => true)).toBe(defaultPort);
  });

  it("skips a stretch of reserved ports and lands on the first free one", async () => {
    // like a Windows machine where 1363-1662 are reserved
    const free = async (port: number) => port < 1363 || port > 1662;
    expect(await findPort(1420, 1000, free)).toBe(1663);
  });

  it("needs the next port free as well, for the phone-on-the-network websocket", async () => {
    const free = async (port: number) => port !== 1421;
    expect(await findPort(1420, 1000, free)).toBe(1422);
  });

  it("gives up with a clear error when nothing in the window is free", async () => {
    await expect(findPort(1420, 5, async () => false)).rejects.toThrow("no free port from 1420 to 1424");
  });
});
