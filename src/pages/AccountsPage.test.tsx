import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { AccountsPage } from "./AccountsPage";
import type { PageDefinition } from "../navigation";

const page: PageDefinition = {
  id: "accounts",
  label: "账户",
  icon: <span />,
  summary: "微软正版账户与离线账户",
  pendingTitle: "",
  pendingBody: "",
};

afterEach(() => {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
});

describe("AccountsPage", () => {
  it("renders both account cards", () => {
    render(<AccountsPage page={page} />);

    expect(
      screen.getByRole("heading", { level: 1, name: "账户" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "微软账户" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "离线账户" }),
    ).toBeInTheDocument();
  });

  it("starts with an empty offline list", () => {
    render(<AccountsPage page={page} />);

    expect(screen.getByText("还没有离线账户")).toBeInTheDocument();
    expect(screen.getByText(/填入玩家名即可添加一个本地账户/)).toBeInTheDocument();
  });

  it("adds an offline account", () => {
    render(<AccountsPage page={page} />);

    fireEvent.change(screen.getByLabelText("玩家名"), {
      target: { value: "Steve" },
    });
    fireEvent.click(screen.getByRole("button", { name: "添加" }));

    expect(screen.getByText("Steve")).toBeInTheDocument();
    expect(screen.getByText("1 个离线账户")).toBeInTheDocument();
  });

  it("rejects an invalid player name", () => {
    render(<AccountsPage page={page} />);

    fireEvent.change(screen.getByLabelText("玩家名"), {
      target: { value: "ab" },
    });
    fireEvent.click(screen.getByRole("button", { name: "添加" }));

    expect(screen.getByText("玩家名至少 3 个字符")).toBeInTheDocument();
    expect(screen.getByText("还没有离线账户")).toBeInTheDocument();
  });

  it("rejects characters outside the allowed set", () => {
    render(<AccountsPage page={page} />);

    fireEvent.change(screen.getByLabelText("玩家名"), {
      target: { value: "bad name" },
    });
    fireEvent.click(screen.getByRole("button", { name: "添加" }));

    expect(
      screen.getByText("只能使用字母、数字和下划线"),
    ).toBeInTheDocument();
  });

  it("removes an added account", () => {
    render(<AccountsPage page={page} />);

    fireEvent.change(screen.getByLabelText("玩家名"), {
      target: { value: "Alex" },
    });
    fireEvent.click(screen.getByRole("button", { name: "添加" }));
    fireEvent.click(screen.getByRole("button", { name: "移除 Alex" }));

    expect(screen.getByText("还没有离线账户")).toBeInTheDocument();
  });

  it("explains why Microsoft login is unavailable in the browser", async () => {
    render(<AccountsPage page={page} />);

    fireEvent.click(screen.getByRole("button", { name: "使用微软账户登录" }));

    await waitFor(() =>
      expect(screen.getByText(/微软登录需要桌面版/)).toBeInTheDocument(),
    );
  });

  it("never renders a token", () => {
    const { container } = render(<AccountsPage page={page} />);

    expect(container.textContent ?? "").not.toMatch(/token/i);
  });
});
