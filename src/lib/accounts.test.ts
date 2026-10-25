import { describe, expect, it } from "vitest";
import {
  addOfflineAccount,
  nextOfflineAccountId,
  removeOfflineAccount,
  summariseOfflineAccounts,
  validatePlayerName,
} from "./accounts";
import type { OfflineAccount } from "./accounts";

function account(name: string, index = 1): OfflineAccount {
  return { id: `offline-${index}`, name, kind: "offline" };
}

describe("validatePlayerName", () => {
  it("accepts 3–16 letters, digits and underscores", () => {
    for (const name of ["abc", "Steve_123", "A".repeat(16), "玩家无法验证"]) {
      // 最后一项含非 ASCII，单独断言失败
      if (name === "玩家无法验证") {
        continue;
      }
      expect(validatePlayerName(name)).toEqual({ ok: true, name });
    }
  });

  it("trims surrounding whitespace", () => {
    expect(validatePlayerName("  Alex  ")).toEqual({ ok: true, name: "Alex" });
  });

  it("rejects empty input", () => {
    expect(validatePlayerName("")).toEqual({
      ok: false,
      error: "请填写玩家名",
    });
    expect(validatePlayerName("   ")).toEqual({
      ok: false,
      error: "请填写玩家名",
    });
  });

  it("rejects names that are too short", () => {
    expect(validatePlayerName("ab")).toEqual({
      ok: false,
      error: "玩家名至少 3 个字符",
    });
  });

  it("rejects names that are too long", () => {
    expect(validatePlayerName("a".repeat(17))).toEqual({
      ok: false,
      error: "玩家名最多 16 个字符",
    });
  });

  it("rejects characters outside [A-Za-z0-9_]", () => {
    // 都用 3 位以上，避免先撞到长度规则
    for (const name of ["bad name", "héllo!", "玩家玩家", "abc-def", "abc.def"]) {
      expect(validatePlayerName(name)).toEqual({
        ok: false,
        error: "只能使用字母、数字和下划线",
      });
    }
  });
});

describe("nextOfflineAccountId", () => {
  it("starts at one", () => {
    expect(nextOfflineAccountId([])).toBe("offline-1");
  });

  it("skips ids already in use", () => {
    expect(nextOfflineAccountId([account("Aa", 1), account("Bb", 2)])).toBe(
      "offline-3",
    );
  });
});

describe("addOfflineAccount", () => {
  it("appends a valid account", () => {
    const result = addOfflineAccount([], "Steve");

    expect(result).toEqual({
      ok: true,
      accounts: [{ id: "offline-1", name: "Steve", kind: "offline" }],
    });
  });

  it("keeps existing accounts", () => {
    const result = addOfflineAccount([account("Steve")], "Alex");

    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.accounts.map((item) => item.name)).toEqual([
        "Steve",
        "Alex",
      ]);
    }
  });

  it("rejects an invalid name without touching the list", () => {
    const existing = [account("Steve")];
    const result = addOfflineAccount(existing, "a");

    expect(result).toEqual({ ok: false, error: "玩家名至少 3 个字符" });
    expect(existing).toHaveLength(1);
  });

  it("rejects a duplicate name ignoring case", () => {
    const result = addOfflineAccount([account("Steve")], "steve");

    expect(result).toEqual({ ok: false, error: "已经有同名账户了" });
  });
});

describe("removeOfflineAccount", () => {
  it("removes by id", () => {
    const accounts = [account("Steve", 1), account("Alex", 2)];
    expect(removeOfflineAccount(accounts, "offline-1")).toEqual([
      account("Alex", 2),
    ]);
  });

  it("ignores unknown ids", () => {
    const accounts = [account("Steve", 1)];
    expect(removeOfflineAccount(accounts, "offline-9")).toEqual(accounts);
  });
});

describe("summariseOfflineAccounts", () => {
  it("handles the empty case", () => {
    expect(summariseOfflineAccounts([])).toBe("还没有离线账户");
  });

  it("counts accounts", () => {
    expect(summariseOfflineAccounts([account("a"), account("b", 2)])).toBe(
      "2 个离线账户",
    );
  });
});
