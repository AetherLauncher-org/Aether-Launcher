import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { pretendBrowser, pretendDesktop, renderWithState, routeInvoke } from "../test/support";
import { DropInstallDialog } from "./DropInstallDialog";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
beforeEach(() => {
  pretendDesktop(); vi.mocked(invoke).mockReset(); vi.mocked(listen).mockClear();
  routeInvoke(vi.mocked(invoke), {
    list_instances: () => ({ instances: [{ id: "i1", name: "Survival", version: "1.21", state: "ready" }], skipped: [] }),
    list_instance_content: () => [{ name: "World", sizeBytes: 0, enabled: true, isFolder: true }],
    install_drop: () => ({ installed: [{ name: "pack.zip", world: "World" }], skipped: [{ name: "mod.jar", world: null, reason: "already exists" }] }),
  });
});
afterEach(pretendBrowser);
it("selects a world and reports installed and skipped files", async () => {
  renderWithState(<DropInstallDialog />);
  await waitFor(() => expect(vi.mocked(listen).mock.calls.some(([name]) => name === "drop://staged")).toBe(true));
  const listener = vi.mocked(listen).mock.calls.find(([name]) => name === "drop://staged")?.[1];
  act(() => { listener?.({ event: "drop://staged", id: 1, payload: { token: "token", items: [{ name: "pack.zip", size: 10, kind: "datapack" }, { name: "mod.jar", size: 10, kind: "mod" }] } }); });
  fireEvent.click(await screen.findByLabelText("World"));
  fireEvent.click(screen.getByRole("button", { name: "安装" }));
  expect(await screen.findByText(/already exists/)).toBeInTheDocument();
  expect(screen.getByText(/pack.zip.*World.*已安装/)).toBeInTheDocument();
  expect(vi.mocked(invoke)).toHaveBeenCalledWith("install_drop", { token: "token", id: "i1", worlds: ["World"] });
});
