import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { LibraryPage } from "./LibraryPage";
import type { PageDefinition } from "../navigation";

const page: PageDefinition = {
  id: "library",
  label: "资源库",
  icon: <span />,
  summary: "模组、资源包与它们的下载来源",
  pendingTitle: "",
  pendingBody: "",
};

describe("LibraryPage", () => {
  it("renders the honest empty state", () => {
    render(<LibraryPage page={page} />);

    expect(
      screen.getByRole("heading", { level: 1, name: "资源库" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "资源来源校验完成后可用" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/下载计划与来源校验还在开发/),
    ).toBeInTheDocument();
  });

  it("keeps the search, source filter and sort controls real", () => {
    render(<LibraryPage page={page} />);

    expect(screen.getByLabelText("搜索资源")).toBeInTheDocument();
    expect(screen.getByLabelText("来源过滤器")).toBeInTheDocument();
    expect(screen.getByLabelText("排序方式")).toBeInTheDocument();
  });

  it("does not fake any rows", () => {
    render(<LibraryPage page={page} />);

    expect(screen.queryAllByRole("row")).toHaveLength(0);
  });

  it("switches to the filtered empty state once a query is typed", () => {
    render(<LibraryPage page={page} />);

    fireEvent.change(screen.getByLabelText("搜索资源"), {
      target: { value: "jei" },
    });

    expect(
      screen.getByRole("heading", { name: "没有匹配的资源" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "清空过滤器" }),
    ).toBeInTheDocument();
  });

  it("clears the filters again", () => {
    render(<LibraryPage page={page} />);

    fireEvent.change(screen.getByLabelText("搜索资源"), {
      target: { value: "jei" },
    });
    fireEvent.click(screen.getByRole("button", { name: "清空过滤器" }));

    expect(
      screen.getByRole("heading", { name: "资源来源校验完成后可用" }),
    ).toBeInTheDocument();
  });

  it("disables the refresh button while there is no source", () => {
    render(<LibraryPage page={page} />);

    expect(screen.getByRole("button", { name: "刷新资源" })).toBeDisabled();
  });
});
