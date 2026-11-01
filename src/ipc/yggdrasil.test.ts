import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  beginYggdrasilLogin,
  cancelYggdrasilLogin,
  describeYggdrasilError,
  finishYggdrasilLogin,
  isYggdrasilServer,
  listYggdrasilAccounts,
  refreshYggdrasilAccount,
  removeYggdrasilAccount,
  yggdrasilPresets,
} from "./yggdrasil";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

describe("Yggdrasil IPC", () => {
  it("sends the login arguments under the command name without retaining the password", async () => {
    vi.mocked(invoke).mockResolvedValue({
      id: "login-1",
      profiles: [{ id: "0123456789abcdef0123456789abcdef", name: "Steve" }],
    });

    await beginYggdrasilLogin("https://littleskin.cn/api/yggdrasil", "user", "hunter2");

    expect(invoke).toHaveBeenCalledWith("begin_yggdrasil_login", {
      server: "https://littleskin.cn/api/yggdrasil",
      username: "user",
      password: "hunter2",
    });
  });

  it("finishes with a camelCase profileId", async () => {
    vi.mocked(invoke).mockResolvedValue(null);
    await finishYggdrasilLogin("login-1", "0123456789abcdef0123456789abcdef");
    expect(invoke).toHaveBeenCalledWith("finish_yggdrasil_login", {
      id: "login-1",
      profileId: "0123456789abcdef0123456789abcdef",
    });
  });

  it("maps list, refresh, remove and cancel to their command names", async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await listYggdrasilAccounts();
    await refreshYggdrasilAccount("a");
    await removeYggdrasilAccount("a");
    await cancelYggdrasilLogin("a");
    expect(invoke).toHaveBeenCalledWith("list_yggdrasil_accounts");
    expect(invoke).toHaveBeenCalledWith("refresh_yggdrasil_account", { id: "a" });
    expect(invoke).toHaveBeenCalledWith("remove_yggdrasil_account", { id: "a" });
    expect(invoke).toHaveBeenCalledWith("cancel_yggdrasil_login", { id: "a" });
  });
});

describe("isYggdrasilServer", () => {
  it("accepts a plain https root without userinfo, query or fragment", () => {
    expect(isYggdrasilServer("https://littleskin.cn/api/yggdrasil")).toBe(true);
  });

  it("rejects non-https and urls carrying credentials or query", () => {
    expect(isYggdrasilServer("http://littleskin.cn")).toBe(false);
    expect(isYggdrasilServer("https://user:pass@example.org")).toBe(false);
    expect(isYggdrasilServer("https://example.org/?token=x")).toBe(false);
    expect(isYggdrasilServer("not a url")).toBe(false);
  });
});

describe("describeYggdrasilError", () => {
  it("translates known stable codes and falls back generically", () => {
    expect(describeYggdrasilError("yggdrasil-invalid-server")).toBe(
      "请输入有效的 HTTPS API 根地址，不要包含账号、查询参数或片段。",
    );
    expect(describeYggdrasilError("yggdrasil-busy")).toContain("已有账户操作");
    expect(describeYggdrasilError("something unknown")).toBe("第三方账户操作失败，请稍后重试。");
    expect(describeYggdrasilError(new Error("yggdrasil-network-error"))).toContain("连接不上");
  });

  it("never echoes the original error body for unknown failures", () => {
    expect(describeYggdrasilError("secret-token")).not.toContain("secret-token");
  });
});

describe("yggdrasilPresets", () => {
  it("uses the verified Ely.by authlib-injector root", () => {
    const ely = yggdrasilPresets.find((preset) => preset.name === "Ely.by");
    expect(ely?.server).toBe("https://account.ely.by/api/authlib-injector");
  });
});
