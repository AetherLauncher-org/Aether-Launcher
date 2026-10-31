import { invoke } from "@tauri-apps/api/core";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { pretendBrowser, pretendDesktop, renderWithState, routeInvoke } from "../test/support";
import type { Handlers } from "../test/support";
import { SkinLibrary } from "./SkinLibrary";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));

const skin = (over: Record<string, unknown> = {}) => ({
  id: "a".repeat(64),
  name: "Hoodie",
  slim: false,
  source: "file",
  added: 1_761_900_000,
  ...over,
});

type Calls = [string, unknown][];
const calls = (name: string): Calls =>
  vi.mocked(invoke).mock.calls.filter(([command]) => command === name) as Calls;

function backend(overrides: Handlers = {}, skins: unknown[] = [skin(), skin({ id: "b".repeat(64), name: "Notch", source: "player" })]) {
  routeInvoke(vi.mocked(invoke), {
    list_skins: () => skins,
    skin_library_image: () => "data:image/png;base64,iVBORw0KGgo=",
    add_skin_from_file: () => null,
    add_skin_from_player: (args) => ({ entry: skin({ name: (args as { name: string }).name }), added: true }),
    rename_skin: (args) => skin({ name: (args as { name: string }).name }),
    set_skin_model: () => skin({ slim: true }),
    remove_skin: () => null,
    ...overrides,
  });
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  pretendDesktop();
});

afterEach(() => {
  pretendBrowser();
});

describe("skin library", () => {
  it("lists the skins with their names and where they came from", async () => {
    backend();
    renderWithState(<SkinLibrary />);

    expect(await screen.findByText("Hoodie")).toBeInTheDocument();
    expect(screen.getByText("Notch")).toBeInTheDocument();
    expect(screen.getByText("文件")).toBeInTheDocument();
    expect(screen.getByText("玩家")).toBeInTheDocument();
    expect(screen.getByText("共 2 张")).toBeInTheDocument();
  });

  it("says so when the library is empty", async () => {
    backend({}, []);
    renderWithState(<SkinLibrary />);

    expect(await screen.findByText("皮肤库是空的")).toBeInTheDocument();
  });

  it("adds a skin from a file through the system dialog and says what was added", async () => {
    backend({ add_skin_from_file: () => ({ entry: skin({ name: "New one" }), added: true }) });
    renderWithState(<SkinLibrary />);
    await screen.findByText("Hoodie");

    fireEvent.click(screen.getByRole("button", { name: "添加皮肤文件" }));

    expect(await screen.findByText("已加进皮肤库：New one")).toBeInTheDocument();
    // the page sends no path: the command takes no arguments
    expect(calls("add_skin_from_file")[0]).toEqual(["add_skin_from_file"]);
  });

  it("says nothing when the dialog is cancelled", async () => {
    backend();
    renderWithState(<SkinLibrary />);
    await screen.findByText("Hoodie");

    fireEvent.click(screen.getByRole("button", { name: "添加皮肤文件" }));

    await waitFor(() => expect(calls("add_skin_from_file")).toHaveLength(1));
    expect(screen.queryByText(/已加进皮肤库/)).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("tells a duplicate from a new skin", async () => {
    backend({ add_skin_from_file: () => ({ entry: skin(), added: false }) });
    renderWithState(<SkinLibrary />);
    await screen.findByText("Hoodie");

    fireEvent.click(screen.getByRole("button", { name: "添加皮肤文件" }));

    expect(await screen.findByText(/已经有这张皮肤了：Hoodie/)).toBeInTheDocument();
  });

  it("adds a player's skin by name, and shows why it failed", async () => {
    backend({
      add_skin_from_player: (args) => {
        if ((args as { name: string }).name === "nobody") {
          throw "no player has this name";
        }
        return { entry: skin({ name: "Notch" }), added: true };
      },
    });
    renderWithState(<SkinLibrary />);
    await screen.findByText("Hoodie");

    fireEvent.click(screen.getByRole("button", { name: "从玩家名添加" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByRole("button", { name: "添加" })).toBeDisabled();
    fireEvent.change(within(dialog).getByRole("textbox"), { target: { value: "nobody" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "添加" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("no player has this name");

    fireEvent.change(within(dialog).getByRole("textbox"), { target: { value: "Notch" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "添加" }));

    await waitFor(() => expect(calls("add_skin_from_player").map(([, args]) => args)).toEqual([{ name: "nobody" }, { name: "Notch" }]));
    expect(await screen.findByText("已加进皮肤库：Notch")).toBeInTheDocument();
  });

  it("renames a skin by its id", async () => {
    backend();
    renderWithState(<SkinLibrary />);
    fireEvent.click(await screen.findByRole("button", { name: "重命名 Hoodie" }));
    const dialog = await screen.findByRole("dialog");

    fireEvent.change(within(dialog).getByRole("textbox"), { target: { value: "Blue hoodie" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "保存" }));

    await waitFor(() => expect(calls("rename_skin")).toHaveLength(1));
    expect(calls("rename_skin")[0][1]).toEqual({ id: "a".repeat(64), name: "Blue hoodie" });
  });

  it("switches a skin to slim arms", async () => {
    backend();
    renderWithState(<SkinLibrary />);

    fireEvent.click(await screen.findByRole("switch", { name: "Hoodie 用纤细手臂" }));

    await waitFor(() => expect(calls("set_skin_model")).toHaveLength(1));
    expect(calls("set_skin_model")[0][1]).toEqual({ id: "a".repeat(64), slim: true });
  });

  it("removes a skin only after confirming", async () => {
    backend();
    renderWithState(<SkinLibrary />);
    fireEvent.click(await screen.findByRole("button", { name: "删除 Hoodie" }));
    expect(calls("remove_skin")).toHaveLength(0);

    fireEvent.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "删除" }));

    await waitFor(() => expect(calls("remove_skin")).toHaveLength(1));
    expect(calls("remove_skin")[0][1]).toEqual({ id: "a".repeat(64) });
  });

  it("does not pretend it can change a skin on an account yet", async () => {
    backend();
    renderWithState(<SkinLibrary />);

    const wear = await screen.findByRole("button", { name: "换上 Hoodie" });

    expect(wear).toHaveAttribute("aria-disabled", "true");
  });
});
