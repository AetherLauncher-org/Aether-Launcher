import { describe, expect, it } from "vitest";
import {
  accountBookStorageKey,
  activeAccount,
  addAccount,
  createOfflineAccount,
  describeUuidMode,
  emptyAccountBook,
  formatUuid,
  loadAccountBook,
  maxPlayerNameLength,
  minPlayerNameLength,
  normalizeUuid,
  parseAccountBook,
  removeAccount,
  saveAccountBook,
  selectAccount,
  summariseAccounts,
  uuidModes,
  validatePlayerName,
} from "./accounts";
import type { AccountBook, OfflineAccount } from "./accounts";

const fixedUuid = "0123456789abcdef0123456789abcdef";

function account(overrides: Partial<OfflineAccount> = {}): OfflineAccount {
  return {
    id: "offline-1",
    kind: "offline",
    name: "Steve",
    uuidMode: "standard",
    uuid: null,
    ...overrides,
  };
}

describe("validatePlayerName", () => {
  it("accepts names inside the length limits", () => {
    expect(validatePlayerName("Steve")).toEqual({ ok: true, name: "Steve" });
    expect(validatePlayerName("a".repeat(minPlayerNameLength)).ok).toBe(true);
    expect(validatePlayerName("a".repeat(maxPlayerNameLength)).ok).toBe(true);
  });

  it("trims whitespace but keeps the casing", () => {
    expect(validatePlayerName("  _Alex_9  ")).toEqual({ ok: true, name: "_Alex_9" });
  });

  it("explains every way a name can be wrong", () => {
    for (const bad of ["", "   ", "ab", "a".repeat(maxPlayerNameLength + 1), "bad name", "名字很好", "a-b"]) {
      const result = validatePlayerName(bad);
      expect(result.ok).toBe(false);
      expect(result.ok ? "" : result.error.length).toBeGreaterThan(0);
    }
  });
});

describe("normalizeUuid and formatUuid", () => {
  it("accepts the plain and dashed forms and lowercases them", () => {
    expect(normalizeUuid("0123456789ABCDEF0123456789abcdef")).toBe(fixedUuid);
    expect(normalizeUuid("01234567-89ab-cdef-0123-456789ABCDEF")).toBe(fixedUuid);
    expect(normalizeUuid(`  ${fixedUuid}  `)).toBe(fixedUuid);
  });

  it("rejects anything else", () => {
    for (const bad of [
      "",
      "not-a-uuid",
      "0123456789abcdef0123456789abcde",
      "0123456789abcdef0123456789abcdefg",
      "0123456-789ab-cdef-0123-456789abcdef0",
      "01234567-89ab-cdef-0123-456789abcdeg",
    ]) {
      expect(normalizeUuid(bad), bad).toBeNull();
    }
  });

  it("formats a compact uuid with dashes and leaves odd input alone", () => {
    expect(formatUuid(fixedUuid)).toBe("01234567-89ab-cdef-0123-456789abcdef");
    expect(formatUuid("short")).toBe("short");
  });
});

describe("createOfflineAccount", () => {
  it("creates a standard account without storing a uuid", () => {
    const result = createOfflineAccount([], { name: " Steve ", mode: "standard", customUuid: "ignored" });
    expect(result).toEqual({
      ok: true,
      account: { id: "offline-1", kind: "offline", name: "Steve", uuidMode: "standard", uuid: null },
    });
  });

  it("stores a generated uuid for the random mode", () => {
    const result = createOfflineAccount([], { name: "Steve", mode: "random", customUuid: "" }, () =>
      "01234567-89AB-CDEF-0123-456789ABCDEF",
    );
    expect(result.ok && result.account.uuid).toBe(fixedUuid);
  });

  it("validates and normalises a custom uuid", () => {
    const good = createOfflineAccount([], {
      name: "Steve",
      mode: "custom",
      customUuid: "01234567-89AB-CDEF-0123-456789ABCDEF",
    });
    expect(good.ok && good.account.uuid).toBe(fixedUuid);

    const bad = createOfflineAccount([], { name: "Steve", mode: "custom", customUuid: "nope" });
    expect(bad).toEqual({ ok: false, errors: { uuid: expect.stringContaining("UUID") } });
  });

  it("reports the name and uuid problems together", () => {
    const result = createOfflineAccount([], { name: "x", mode: "custom", customUuid: "" });
    expect(result.ok).toBe(false);
    expect(result.ok ? null : Object.keys(result.errors).sort()).toEqual(["name", "uuid"]);
  });

  it("refuses a duplicate name regardless of case", () => {
    const result = createOfflineAccount([account()], { name: "STEVE", mode: "standard", customUuid: "" });
    expect(result).toEqual({ ok: false, errors: { name: "已经有同名账户了" } });
  });

  it("picks an id that does not collide with existing ones", () => {
    const result = createOfflineAccount([account({ id: "offline-2" })], {
      name: "Alex",
      mode: "standard",
      customUuid: "",
    });
    expect(result.ok && result.account.id).toBe("offline-3");
  });
});

describe("account book operations", () => {
  const steve = account();
  const alex = account({ id: "offline-2", name: "Alex" });

  it("makes the first account active and keeps the active one when adding more", () => {
    const first = addAccount(emptyAccountBook, steve);
    expect(first).toEqual({ accounts: [steve], activeId: "offline-1" });
    expect(addAccount(first, alex).activeId).toBe("offline-1");
  });

  it("falls back to the first remaining account when the active one is removed", () => {
    const book: AccountBook = { accounts: [steve, alex], activeId: "offline-1" };
    expect(removeAccount(book, "offline-1")).toEqual({ accounts: [alex], activeId: "offline-2" });
    expect(removeAccount(book, "offline-2").activeId).toBe("offline-1");
    expect(removeAccount({ accounts: [steve], activeId: "offline-1" }, "offline-1")).toEqual(emptyAccountBook);
  });

  it("selects an existing account and ignores an unknown id", () => {
    const book: AccountBook = { accounts: [steve, alex], activeId: "offline-1" };
    expect(selectAccount(book, "offline-2").activeId).toBe("offline-2");
    expect(selectAccount(book, "missing")).toBe(book);
  });

  it("finds the active account", () => {
    expect(activeAccount({ accounts: [steve, alex], activeId: "offline-2" })).toBe(alex);
    expect(activeAccount(emptyAccountBook)).toBeNull();
  });

  it("summarises the list", () => {
    expect(summariseAccounts([])).toBe("还没有账户");
    expect(summariseAccounts([steve, alex])).toBe("2 个离线账户");
  });

  it("describes every uuid mode", () => {
    for (const mode of uuidModes) {
      expect(describeUuidMode(mode).label.length).toBeGreaterThan(0);
      expect(describeUuidMode(mode).description.length).toBeGreaterThan(0);
    }
  });
});

describe("parseAccountBook", () => {
  it("returns an empty book for missing, malformed or non-object data", () => {
    expect(parseAccountBook(null)).toEqual(emptyAccountBook);
    expect(parseAccountBook("{not json")).toEqual(emptyAccountBook);
    expect(parseAccountBook("42")).toEqual(emptyAccountBook);
    expect(parseAccountBook("null")).toEqual(emptyAccountBook);
  });

  it("keeps valid accounts and drops damaged ones", () => {
    const raw = JSON.stringify({
      accounts: [
        account(),
        { id: "x", kind: "offline", name: "ab", uuidMode: "standard", uuid: null },
        { id: "y", kind: "microsoft", name: "Alex", uuidMode: "standard", uuid: null },
        { id: "z", kind: "offline", name: "Alex", uuidMode: "custom", uuid: "bad" },
        { id: "w", kind: "offline", name: "Alex", uuidMode: "wrong", uuid: null },
        "text",
        null,
      ],
      activeId: "offline-1",
    });
    expect(parseAccountBook(raw).accounts).toEqual([account()]);
  });

  it("keeps only the first account when ids repeat", () => {
    const raw = JSON.stringify({ accounts: [account(), account({ name: "Other" })], activeId: "offline-1" });
    expect(parseAccountBook(raw).accounts).toHaveLength(1);
  });

  it("repairs an active id that points nowhere", () => {
    const raw = JSON.stringify({ accounts: [account()], activeId: "gone" });
    expect(parseAccountBook(raw).activeId).toBe("offline-1");
    expect(parseAccountBook(JSON.stringify({ accounts: [], activeId: "gone" })).activeId).toBeNull();
  });

  it("normalises a stored uuid", () => {
    const raw = JSON.stringify({
      accounts: [account({ uuidMode: "custom", uuid: "01234567-89AB-CDEF-0123-456789ABCDEF" })],
      activeId: "offline-1",
    });
    expect(parseAccountBook(raw).accounts[0].uuid).toBe(fixedUuid);
  });
});

describe("account book storage", () => {
  it("round-trips through localStorage", () => {
    const book: AccountBook = {
      accounts: [account(), account({ id: "offline-2", name: "Alex", uuidMode: "random", uuid: fixedUuid })],
      activeId: "offline-2",
    };
    saveAccountBook(book);
    expect(window.localStorage.getItem(accountBookStorageKey)).not.toBeNull();
    expect(loadAccountBook()).toEqual(book);
  });

  it("never stores anything that looks like a token", () => {
    saveAccountBook({ accounts: [account()], activeId: "offline-1" });
    expect(window.localStorage.getItem(accountBookStorageKey)).not.toMatch(/token/i);
  });

  it("starts empty when nothing was saved", () => {
    expect(loadAccountBook()).toEqual(emptyAccountBook);
  });
});
