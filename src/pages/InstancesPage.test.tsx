import { invoke } from "@tauri-apps/api/core";
import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { InstancesPage } from "./InstancesPage";
import type { PageDefinition } from "../navigation";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const page: PageDefinition = {
  id: "instances",
  label: "实例",
  icon: <span />,
  summary: "每个实例的版本、模组、Java 与启动参数相互隔离",
  pendingTitle: "",
  pendingBody: "",
};

function enableTauri() {
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {},
    configurable: true,
  });
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

afterEach(() => {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
});

describe("InstancesPage", () => {
  it("renders the empty state in a plain browser", () => {
    render(<InstancesPage page={page} />);

    expect(screen.getByRole("heading", { level: 1, name: "实例" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "还没有实例" })).toBeInTheDocument();
    expect(screen.getByText(/浏览器预览读不到本机实例/)).toBeInTheDocument();
  });

  it("explains that importing is not available yet", () => {
    render(<InstancesPage page={page} />);

    expect(screen.getByRole("button", { name: "导入实例" })).toBeDisabled();
  });

  it("lists instances returned by the backend", async () => {
    enableTauri();
    vi.mocked(invoke).mockResolvedValue({
      instances: [
        { id: "a", name: "生存 1.21", version: "1.21.1", state: "ready" },
        { id: "b", name: "红石实验", version: "1.20.1", state: "running" },
      ],
      skipped: [],
    });

    render(<InstancesPage page={page} />);

    await waitFor(() =>
      expect(screen.getByText("生存 1.21")).toBeInTheDocument(),
    );
    expect(screen.getByText("红石实验")).toBeInTheDocument();
    expect(screen.getByText("就绪")).toBeInTheDocument();
    expect(screen.getByText("运行中")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("list_instances");
  });

  it("shows an error state with a retry button", async () => {
    enableTauri();
    vi.mocked(invoke).mockRejectedValue("实例目录不可读");

    render(<InstancesPage page={page} />);

    await waitFor(() =>
      expect(screen.getByRole("heading", { name: "读取实例失败" })).toBeInTheDocument(),
    );
    expect(screen.getByText("实例目录不可读")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重新读取" })).toBeInTheDocument();
  });

  it("reports records the backend refused to load", async () => {
    enableTauri();
    vi.mocked(invoke).mockResolvedValue({
      instances: [],
      skipped: [{ id: "broken", reason: "invalid-json" }],
    });

    render(<InstancesPage page={page} />);

    await waitFor(() =>
      expect(screen.getByText(/有 1 条记录没读出来/)).toBeInTheDocument(),
    );
    expect(screen.getByText(/实例文件格式损坏/)).toBeInTheDocument();
  });

  it("filters the list by name", async () => {
    enableTauri();
    vi.mocked(invoke).mockResolvedValue({
      instances: [
        { id: "a", name: "生存 1.21", version: "1.21.1", state: "ready" },
        { id: "b", name: "红石实验", version: "1.20.1", state: "ready" },
      ],
      skipped: [],
    });

    const { findByLabelText, findByText } = render(
      <InstancesPage page={page} />,
    );

    const search = await findByLabelText("筛选实例");
    const { fireEvent } = await import("@testing-library/react");
    fireEvent.change(search, { target: { value: "红石" } });

    expect(await findByText("红石实验")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.queryByText("生存 1.21")).not.toBeInTheDocument(),
    );
  });
});
