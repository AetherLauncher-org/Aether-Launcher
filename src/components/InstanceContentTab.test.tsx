import { FluentProvider, webLightTheme } from "@fluentui/react-components";
import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { contentLookupStorageKey } from "../lib/contentLookup";
import { pretendBrowser, pretendDesktop, routeInvoke } from "../test/support";
import type { Handlers } from "../test/support";
import { InstanceContentTab } from "./InstanceContentTab";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));

const mods = [
  { name: "sodium-0.6.jar", sizeBytes: 1_300_000, enabled: true, isFolder: false },
  { name: "my-own-build.jar", sizeBytes: 5_000, enabled: true, isFolder: false },
];

const sodium = {
  name: "sodium-0.6.jar",
  platform: "modrinth",
  projectSlug: "sodium",
  title: "Sodium",
  summary: "A modern rendering engine",
  version: "0.6.3",
  icon: "data:image/png;base64,AAAA",
};

function calls(name: string) {
  return vi.mocked(invoke).mock.calls.filter(([command]) => command === name) as [string, unknown][];
}

function backend(overrides: Handlers = {}) {
  routeInvoke(vi.mocked(invoke), {
    list_instance_content: (args) => ((args as { kind: string }).kind === "mods" ? mods : []),
    identify_instance_content: () => ({ items: [sodium], unreachable: false }),
    open_content_project: () => null,
    ...overrides,
  });
}

function show(kind: "mods" | "saves" = "mods") {
  return render(
    <FluentProvider theme={webLightTheme}>
      <InstanceContentTab instanceId="i1" kind={kind} locked={false} onChanged={() => undefined} />
    </FluentProvider>,
  );
}

beforeEach(() => {
  pretendDesktop();
  window.localStorage.clear();
});

afterEach(() => {
  pretendBrowser();
  vi.restoreAllMocks();
});

describe("content tab with Modrinth lookup", () => {
  it("shows the project's name, version and summary, and keeps the file name small", async () => {
    backend();
    show();

    expect(await screen.findByText("Sodium")).toBeInTheDocument();
    expect(screen.getByText("0.6.3")).toBeInTheDocument();
    expect(screen.getByText("A modern rendering engine")).toBeInTheDocument();
    expect(screen.getByText("sodium-0.6")).toBeInTheDocument();
    // 没认出来的文件照旧显示文件名
    expect(screen.getByText("my-own-build")).toBeInTheDocument();
  });

  it("sends only the instance and the kind, never a file name or a path", async () => {
    backend();
    show();
    await screen.findByText("Sodium");

    expect(calls("identify_instance_content")[0][1]).toEqual({ id: "i1", kind: "mods" });
  });

  it("keeps the delete and switch labels on the file name", async () => {
    backend();
    show();
    await screen.findByText("Sodium");

    expect(screen.getByRole("button", { name: "删除 sodium-0.6" })).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "启用 sodium-0.6" })).toBeInTheDocument();
  });

  it("opens the project page by its slug", async () => {
    backend();
    show();

    fireEvent.click(await screen.findByRole("button", { name: "在 Modrinth 打开 Sodium" }));

    await waitFor(() => expect(calls("open_content_project")).toHaveLength(1));
    expect(calls("open_content_project")[0][1]).toEqual({ slug: "sodium" });
  });

  it("shows a refusal to open the page", async () => {
    backend({
      open_content_project: () => {
        throw "cannot open the default browser";
      },
    });
    show();

    fireEvent.click(await screen.findByRole("button", { name: "在 Modrinth 打开 Sodium" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("cannot open the default browser");
  });

  it("says when it is showing the cache because Modrinth could not be reached", async () => {
    backend({ identify_instance_content: () => ({ items: [sodium], unreachable: true }) });
    show();

    expect(await screen.findByText(/没连上 Modrinth/)).toBeInTheDocument();
    expect(screen.getByText("Sodium")).toBeInTheDocument();
  });

  it("still lists the files when the lookup fails", async () => {
    backend({
      identify_instance_content: () => {
        throw new Error("boom");
      },
    });
    show();

    expect(await screen.findByText("sodium-0.6")).toBeInTheDocument();
    expect(screen.getByText("my-own-build")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("stops asking and goes back to file names when the lookup is switched off", async () => {
    backend();
    show();
    await screen.findByText("Sodium");

    fireEvent.click(screen.getByRole("switch", { name: "联网识别" }));

    await waitFor(() => expect(screen.queryByText("Sodium")).toBeNull());
    expect(screen.getByText("sodium-0.6")).toBeInTheDocument();
    expect(window.localStorage.getItem(contentLookupStorageKey)).toBe("off");
  });

  it("makes no request at all when the lookup was switched off earlier", async () => {
    window.localStorage.setItem(contentLookupStorageKey, "off");
    backend();
    show();

    expect(await screen.findByText("sodium-0.6")).toBeInTheDocument();
    expect(calls("identify_instance_content")).toHaveLength(0);
    expect(screen.getByRole("switch", { name: "联网识别" })).not.toBeChecked();
  });

  it("does not look worlds up", async () => {
    backend({
      list_instance_content: () => [{ name: "My World", sizeBytes: 1000, enabled: true, isFolder: true }],
    });
    show("saves");

    expect(await screen.findByText("My World")).toBeInTheDocument();
    expect(calls("identify_instance_content")).toHaveLength(0);
    expect(screen.queryByRole("switch", { name: "联网识别" })).toBeNull();
  });
});
