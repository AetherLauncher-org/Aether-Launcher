import { FluentProvider, webLightTheme } from "@fluentui/react-components";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { ContentItem } from "../ipc";
import { describeShape } from "../lib/instanceContent";
import { ContentCards } from "./ContentCards";

const jar: ContentItem = { name: "sodium.jar", sizeBytes: 1536, isFolder: false, enabled: true };
const off: ContentItem = { name: "lithium.jar.disabled", sizeBytes: 2048, isFolder: false, enabled: false };
const world: ContentItem = { name: "My World", sizeBytes: 4096, isFolder: true, enabled: true };

function show(kind: "mods" | "saves" | "resourcepacks", items: ContentItem[], locked = false) {
  const onToggle = vi.fn();
  const onRemove = vi.fn();
  render(
    <FluentProvider theme={webLightTheme}>
      <ContentCards kind={kind} items={items} locked={locked} onToggle={onToggle} onRemove={onRemove} />
    </FluentProvider>,
  );
  return { onToggle, onRemove };
}

describe("describeShape", () => {
  it("names the form of each kind", () => {
    expect(describeShape("mods", jar)).toBe("JAR");
    expect(describeShape("resourcepacks", jar)).toBe("压缩包");
    expect(describeShape("saves", world)).toBe("世界");
    expect(describeShape("shaderpacks", world)).toBe("文件夹");
  });
});

describe("ContentCards", () => {
  it("renders one card per item with its size", () => {
    show("mods", [jar, off]);
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
    expect(screen.getByText("1.5 KB")).toBeInTheDocument();
  });

  it("marks a disabled mod once and toggles it by its plain file name", () => {
    const { onToggle } = show("mods", [off]);
    expect(screen.getAllByText("已关闭")).toHaveLength(1);
    fireEvent.click(screen.getByRole("switch", { name: /启用/ }));
    expect(onToggle).toHaveBeenCalledWith(off, true);
  });

  it("has no switch for kinds other than mods", () => {
    show("saves", [world]);
    expect(screen.queryByRole("switch")).toBeNull();
  });

  it("asks to remove an item", () => {
    const { onRemove } = show("saves", [world]);
    fireEvent.click(screen.getByRole("button", { name: "删除 My World" }));
    expect(onRemove).toHaveBeenCalledWith(world);
  });

  it("replaces the name, cover and facts with the platform's when a file is identified", () => {
    const found = {
      name: "sodium.jar",
      platform: "modrinth" as const,
      projectSlug: "sodium",
      title: "Sodium",
      summary: "Fast rendering",
      version: "0.6.3",
      icon: "data:image/png;base64,AAAA",
    };
    const onOpenProject = vi.fn();
    render(
      <FluentProvider theme={webLightTheme}>
        <ContentCards
          kind="mods"
          items={[jar]}
          identified={new Map([["sodium.jar", found]])}
          locked={false}
          onToggle={vi.fn()}
          onRemove={vi.fn()}
          onOpenProject={onOpenProject}
        />
      </FluentProvider>,
    );

    expect(screen.getByText("Sodium")).toBeInTheDocument();
    expect(screen.getByText("Fast rendering")).toBeInTheDocument();
    expect(screen.getByText("0.6.3")).toBeInTheDocument();
    expect(screen.getByText("sodium")).toBeInTheDocument();
    expect(document.querySelector(".content-cover img")).toHaveAttribute("src", found.icon);

    fireEvent.click(screen.getByRole("button", { name: "在 Modrinth 打开 Sodium" }));
    expect(onOpenProject).toHaveBeenCalledWith(found);
  });

  it("shows no platform button for a file that was not identified", () => {
    show("mods", [jar]);
    expect(screen.queryByRole("button", { name: /Modrinth/ })).toBeNull();
    expect(document.querySelector(".content-cover img")).toBeNull();
  });

  it("disables switch and delete while the game runs", () => {
    show("mods", [jar], true);
    expect(screen.getByRole("switch")).toBeDisabled();
    expect(screen.getByRole("button", { name: /删除/ })).toBeDisabled();
  });
});
