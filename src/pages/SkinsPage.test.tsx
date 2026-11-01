import { invoke } from "@tauri-apps/api/core";
import { screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { findPage } from "../navigation";
import { pretendBrowser, pretendDesktop, renderWithState, routeInvoke } from "../test/support";
import { SkinsPage } from "./SkinsPage";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));

describe("SkinsPage", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    pretendDesktop();
    routeInvoke(vi.mocked(invoke), {
      list_skins: () => [
        { id: "a".repeat(64), name: "Hoodie", slim: false, source: "file", added: 1_761_900_000 },
      ],
      skin_library_image: () => "data:image/png;base64,iVBORw0KGgo=",
    });
  });

  afterEach(() => {
    pretendBrowser();
  });

  it("is its own page with a title and the skin library, not an accounts tab", async () => {
    renderWithState(<SkinsPage page={findPage("skins")} />);

    expect(screen.getByRole("heading", { name: "皮肤库" })).toBeInTheDocument();
    expect(await screen.findByText("Hoodie")).toBeInTheDocument();
    expect(screen.queryByRole("tab")).not.toBeInTheDocument();
  });
});
