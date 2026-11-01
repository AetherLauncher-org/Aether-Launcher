import { invoke } from "@tauri-apps/api/core";
import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { pretendBrowser, pretendDesktop, renderWithState, routeInvoke } from "../test/support";
import type { Handlers } from "../test/support";
import { SkinSources } from "./SkinSources";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));

const preview = {
  token: "opaque-preview-token",
  source: "littleskin",
  name: "Notch",
  slim: true,
  image: "data:image/png;base64,iVBORw0KGgo=",
};

const skin = {
  id: "a".repeat(64),
  name: "Notch",
  slim: true,
  source: "download",
  added: 1_761_900_000,
};

type Calls = [string, unknown][];
const calls = (name: string): Calls =>
  vi.mocked(invoke).mock.calls.filter(([command]) => command === name) as Calls;

function backend(overrides: Handlers = {}) {
  routeInvoke(vi.mocked(invoke), {
    search_skin: () => preview,
    add_downloaded_skin: () => ({ entry: skin, added: true }),
    save_downloaded_skin: () => ({ fileName: "Notch.png" }),
    open_skin_site: () => null,
    add_skin_from_file: () => ({ entry: skin, added: true }),
    ...overrides,
  });
}

async function searchForNotch() {
  fireEvent.change(screen.getByLabelText("玩家名"), { target: { value: "Notch" } });
  fireEvent.click(screen.getByRole("button", { name: "搜索" }));
  expect(await screen.findByRole("heading", { name: "Notch" })).toBeInTheDocument();
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  pretendDesktop();
});

afterEach(() => {
  pretendBrowser();
});

describe("skin sources", () => {
  it("rejects empty or invalid names and searches the selected source", async () => {
    backend();
    renderWithState(<SkinSources />);

    const search = screen.getByRole("button", { name: "搜索" });
    expect(search).toBeDisabled();
    fireEvent.change(screen.getByLabelText("玩家名"), { target: { value: "not a name" } });
    expect(search).toBeDisabled();
    expect(screen.getByText(/玩家名需为 3–16/)).toBeInTheDocument();
    expect(calls("search_skin")).toHaveLength(0);

    fireEvent.change(screen.getByRole("combobox", { name: "皮肤来源" }), { target: { value: "littleskin" } });
    await searchForNotch();

    expect(calls("search_skin")[0]).toEqual(["search_skin", { source: "littleskin", name: "Notch" }]);
    expect(screen.getAllByText("LittleSkin")).toHaveLength(2);
    expect(screen.getByText("纤细手臂")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Notch 的全身皮肤预览" })).toHaveProperty("tagName", "CANVAS");
    expect(screen.getByRole("button", { name: "加入皮肤库" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "保存到文件夹…" })).toBeEnabled();
  });

  it("locks the controls while a search is pending", async () => {
    let finish: (value: typeof preview) => void = () => undefined;
    backend({ search_skin: () => new Promise<typeof preview>((resolve) => (finish = resolve)) });
    renderWithState(<SkinSources />);

    fireEvent.change(screen.getByLabelText("玩家名"), { target: { value: "Notch" } });
    fireEvent.click(screen.getByRole("button", { name: "搜索" }));

    expect(screen.getByRole("button", { name: "搜索" })).toBeDisabled();
    expect(screen.getByLabelText("玩家名")).toBeDisabled();
    expect(screen.getByRole("button", { name: "添加皮肤文件" })).toBeDisabled();
    expect(calls("search_skin")).toHaveLength(1);

    await act(async () => finish(preview));
    expect(await screen.findByRole("heading", { name: "Notch" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "搜索" })).toBeEnabled();
  });

  it("shows a search failure and can retry", async () => {
    let attempts = 0;
    backend({
      search_skin: () => {
        attempts += 1;
        if (attempts === 1) {
          throw "profile service unavailable";
        }
        return preview;
      },
    });
    renderWithState(<SkinSources />);
    fireEvent.change(screen.getByLabelText("玩家名"), { target: { value: "Notch" } });

    fireEvent.click(screen.getByRole("button", { name: "搜索" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("profile service unavailable");

    fireEvent.click(screen.getByRole("button", { name: "搜索" }));
    expect(await screen.findByRole("heading", { name: "Notch" })).toBeInTheDocument();
    expect(attempts).toBe(2);
  });

  it("adds the opaque preview token and distinguishes duplicates", async () => {
    let additions = 0;
    backend({
      add_downloaded_skin: () => {
        additions += 1;
        return { entry: skin, added: additions === 1 };
      },
    });
    renderWithState(<SkinSources />);
    await searchForNotch();

    fireEvent.click(screen.getByRole("button", { name: "加入皮肤库" }));
    expect(await screen.findByText("已加进皮肤库：Notch")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "加入皮肤库" }));
    expect(await screen.findByText(/已经有这张皮肤了：Notch/)).toBeInTheDocument();

    expect(calls("add_downloaded_skin")).toEqual([
      ["add_downloaded_skin", { token: "opaque-preview-token" }],
      ["add_downloaded_skin", { token: "opaque-preview-token" }],
    ]);
  });

  it("handles folder picker cancellation and then reports a successful save", async () => {
    let saved: { fileName: string } | null = null;
    backend({ save_downloaded_skin: () => saved });
    renderWithState(<SkinSources />);
    await searchForNotch();

    fireEvent.click(screen.getByRole("button", { name: "保存到文件夹…" }));
    await waitFor(() => expect(calls("save_downloaded_skin")).toHaveLength(1));
    expect(screen.queryByText(/已保存 PNG/)).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();

    saved = { fileName: "Notch.png" };
    fireEvent.click(screen.getByRole("button", { name: "保存到文件夹…" }));
    expect(await screen.findByText("已保存 PNG：Notch.png")).toBeInTheDocument();
    expect(calls("save_downloaded_skin").map(([, args]) => args)).toEqual([
      { token: "opaque-preview-token" },
      { token: "opaque-preview-token" },
    ]);
  });

  it("opens each community site through its fixed key", async () => {
    backend();
    renderWithState(<SkinSources />);

    for (const label of ["NameMC", "Skindex", "Planet Minecraft"]) {
      const button = screen.getByRole("button", { name: `在浏览器里打开 ${label}` });
      fireEvent.click(button);
      await waitFor(() => expect(button).toBeEnabled());
    }

    expect(calls("open_skin_site").map(([, args]) => args)).toEqual([
      { site: "namemc" },
      { site: "skindex" },
      { site: "planetminecraft" },
    ]);
  });

  it("adds a local PNG without sending a file path", async () => {
    backend({ add_skin_from_file: () => ({ entry: { ...skin, name: "Local skin" }, added: true }) });
    renderWithState(<SkinSources />);

    fireEvent.click(screen.getByRole("button", { name: "添加皮肤文件" }));

    expect(await screen.findByText("已加进皮肤库：Local skin")).toBeInTheDocument();
    expect(calls("add_skin_from_file")[0]).toEqual(["add_skin_from_file"]);
  });
});
