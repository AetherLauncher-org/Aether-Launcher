/**
 * 账户的纯逻辑：玩家名与 UUID 校验、离线账户构造、账户簿的读写。
 *
 * 规则与 Java 版用户名一致：3–16 位，只允许字母、数字和下划线。
 * 离线账户没有任何秘密：只有玩家名和 UUID，所以可以明文存在 `localStorage`。
 * 微软账户在这里只有身份（名字和 UUID），令牌不经过这里：它们由后端保管，
 * 真正的账户列表以后端为准，启动时用 `syncMicrosoftAccounts` 对齐。
 */

export const minPlayerNameLength = 3;

export const maxPlayerNameLength = 16;

const allowedPattern = /^[A-Za-z0-9_]+$/;

export type PlayerNameValidation =
  | { ok: true; name: string }
  | { ok: false; error: string };

/**
 * 校验玩家名。返回规范化后的名字（原样保留大小写），
 * 或者一句可以直接显示在输入框下方的错误。
 */
export function validatePlayerName(raw: string): PlayerNameValidation {
  const name = raw.trim();

  if (name.length === 0) {
    return { ok: false, error: "请填写用户名" };
  }
  if (name.length < minPlayerNameLength) {
    return { ok: false, error: `用户名至少 ${minPlayerNameLength} 个字符` };
  }
  if (name.length > maxPlayerNameLength) {
    return { ok: false, error: `用户名最多 ${maxPlayerNameLength} 个字符` };
  }
  if (!allowedPattern.test(name)) {
    return { ok: false, error: "只能使用字母、数字和下划线" };
  }
  return { ok: true, name };
}

/**
 * UUID 的生成方式。
 * - `standard`：由用户名派生的标准离线 UUID，与官方离线模式一致，同名同 UUID（后端计算）。
 * - `random`：创建时随机生成并保存，之后固定不变。
 * - `custom`：用户自己指定，用于和已有存档或服务器白名单对上。
 */
export type UuidMode = "standard" | "random" | "custom";

export const uuidModes: readonly UuidMode[] = ["standard", "random", "custom"];

export type UuidModeInfo = { label: string; description: string };

const modeInfo: Record<UuidMode, UuidModeInfo> = {
  standard: {
    label: "标准（由用户名生成）",
    description: "与官方离线模式一致：同一个用户名永远得到同一个 UUID，存档和背包不会丢。",
  },
  random: {
    label: "随机",
    description: "创建时随机生成一个 UUID 并保存。适合需要和用户名无关的身份。",
  },
  custom: {
    label: "自定义",
    description: "手动填写 UUID，用于匹配已有的存档或服务器白名单。",
  },
};

export function describeUuidMode(mode: UuidMode): UuidModeInfo {
  return modeInfo[mode];
}

export function isUuidMode(value: unknown): value is UuidMode {
  return value === "standard" || value === "random" || value === "custom";
}

/**
 * 把 UUID 规范成 32 位小写十六进制。接受带连字符（8-4-4-4-12）和不带连字符两种写法，
 * 其他写法返回 null。规则与后端的 `normalize_uuid` 完全一致。
 */
export function normalizeUuid(raw: string): string | null {
  const text = raw.trim();
  let compact = text;
  if (text.length === 36) {
    const groups = text.split("-");
    if (groups.map((group) => group.length).join(",") !== "8,4,4,4,12") {
      return null;
    }
    compact = groups.join("");
  }
  return /^[0-9a-fA-F]{32}$/.test(compact) ? compact.toLowerCase() : null;
}

/** `0123…cdef` → `01234567-89ab-cdef-0123-456789abcdef`，用于显示。 */
export function formatUuid(compact: string): string {
  if (compact.length !== 32) {
    return compact;
  }
  return [
    compact.slice(0, 8),
    compact.slice(8, 12),
    compact.slice(12, 16),
    compact.slice(16, 20),
    compact.slice(20),
  ].join("-");
}

/** 离线账户。只有展示和启动需要的信息——没有 token，也没有密码字段。 */
export type OfflineAccount = {
  id: string;
  kind: "offline";
  name: string;
  uuidMode: UuidMode;
  /** `standard` 时为 null（后端派生）；其余模式是 32 位小写十六进制 */
  uuid: string | null;
};

/** 微软账户的身份。没有任何令牌字段；`id` 是后端的账户 ID（Minecraft 档案 UUID）。 */
export type MicrosoftAccountEntry = {
  id: string;
  kind: "microsoft";
  name: string;
  /** 32 位小写十六进制 */
  uuid: string;
};

export type Account = OfflineAccount | MicrosoftAccountEntry;

export type AccountDraft = {
  name: string;
  mode: UuidMode;
  customUuid: string;
};

export type AccountDraftErrors = { name?: string; uuid?: string };

export type AccountCreation =
  | { ok: true; account: OfflineAccount }
  | { ok: false; errors: AccountDraftErrors };

/** 随机 UUID 的来源；测试里注入固定值。 */
export type UuidGenerator = () => string;

const defaultGenerator: UuidGenerator = () => crypto.randomUUID();

/**
 * 校验草稿并构造账户。名字不合法、同名（不区分大小写）、自定义 UUID 不合法
 * 都会返回对应字段的错误；原列表不会被改动。
 */
export function createOfflineAccount(
  accounts: readonly Account[],
  draft: AccountDraft,
  generate: UuidGenerator = defaultGenerator,
): AccountCreation {
  const errors: AccountDraftErrors = {};

  const nameCheck = validatePlayerName(draft.name);
  if (!nameCheck.ok) {
    errors.name = nameCheck.error;
  } else if (
    accounts.some((account) => account.name.toLowerCase() === nameCheck.name.toLowerCase())
  ) {
    errors.name = "已经有同名账户了";
  }

  let uuid: string | null = null;
  if (draft.mode === "custom") {
    uuid = normalizeUuid(draft.customUuid);
    if (uuid === null) {
      errors.uuid = "UUID 需要是 32 位十六进制，可以带连字符";
    }
  } else if (draft.mode === "random") {
    uuid = normalizeUuid(generate());
  }

  if (errors.name !== undefined || errors.uuid !== undefined || !nameCheck.ok) {
    return { ok: false, errors };
  }

  return {
    ok: true,
    account: {
      id: nextAccountId(accounts),
      kind: "offline",
      name: nameCheck.name,
      uuidMode: draft.mode,
      uuid,
    },
  };
}

/** 递增序号 ID：账户只存在本机，序号足够区分，且删除后不会复用同时存在的 ID。 */
export function nextAccountId(existing: readonly Account[]): string {
  const used = new Set(existing.map((account) => account.id));
  let index = existing.length + 1;
  while (used.has(`offline-${index}`)) {
    index += 1;
  }
  return `offline-${index}`;
}

/** 账户簿：全部账户和当前选中的那个。 */
export type AccountBook = {
  accounts: Account[];
  activeId: string | null;
};

export const emptyAccountBook: AccountBook = { accounts: [], activeId: null };

export const accountBookStorageKey = "aether.accounts";

/** 添加账户；第一个账户自动成为当前账户。 */
export function addAccount(book: AccountBook, account: Account): AccountBook {
  return {
    accounts: [...book.accounts, account],
    activeId: book.activeId ?? account.id,
  };
}

/** 移除账户；删掉的是当前账户时，改选第一个剩下的。 */
export function removeAccount(book: AccountBook, id: string): AccountBook {
  const accounts = book.accounts.filter((account) => account.id !== id);
  const activeId =
    book.activeId !== null && accounts.some((account) => account.id === book.activeId)
      ? book.activeId
      : (accounts[0]?.id ?? null);
  return { accounts, activeId };
}

/** 选择当前账户；ID 不存在时保持原样。 */
export function selectAccount(book: AccountBook, id: string): AccountBook {
  return book.accounts.some((account) => account.id === id) ? { ...book, activeId: id } : book;
}

export function activeAccount(book: AccountBook): Account | null {
  return book.accounts.find((account) => account.id === book.activeId) ?? null;
}

/** 把后端的账户列表对齐到账户簿：后端没有的微软账户删掉，新的加进来，名字以后端为准。 */
export function syncMicrosoftAccounts(
  book: AccountBook,
  signedIn: readonly MicrosoftAccountEntry[],
): AccountBook {
  const byId = new Map(signedIn.map((account) => [account.id, account]));
  const kept: Account[] = [];
  const seen = new Set<string>();
  for (const account of book.accounts) {
    if (account.kind !== "microsoft") {
      kept.push(account);
    } else {
      const fresh = byId.get(account.id);
      if (fresh !== undefined) {
        kept.push(fresh);
        seen.add(fresh.id);
      }
    }
  }
  for (const account of signedIn) {
    if (!seen.has(account.id)) {
      kept.push(account);
    }
  }
  const activeId =
    book.activeId !== null && kept.some((account) => account.id === book.activeId)
      ? book.activeId
      : (kept[0]?.id ?? null);
  return { accounts: kept, activeId };
}

/** 添加或更新一个刚登录的微软账户，并把它设为当前账户。 */
export function signInMicrosoftAccount(
  book: AccountBook,
  account: MicrosoftAccountEntry,
): AccountBook {
  const others = book.accounts.filter((existing) => existing.id !== account.id);
  return { accounts: [...others, account], activeId: account.id };
}

function parseAccount(value: unknown): Account | null {
  if (typeof value !== "object" || value === null) {
    return null;
  }
  const record = value as Record<string, unknown>;
  if (record.kind === "microsoft") {
    const uuid = typeof record.uuid === "string" ? normalizeUuid(record.uuid) : null;
    if (
      typeof record.id !== "string" ||
      typeof record.name !== "string" ||
      record.name.length === 0 ||
      uuid === null
    ) {
      return null;
    }
    return { id: record.id, kind: "microsoft", name: record.name, uuid };
  }
  if (
    typeof record.id !== "string" ||
    record.kind !== "offline" ||
    typeof record.name !== "string" ||
    !isUuidMode(record.uuidMode) ||
    !validatePlayerName(record.name).ok
  ) {
    return null;
  }
  let uuid: string | null = null;
  if (record.uuidMode !== "standard") {
    uuid = typeof record.uuid === "string" ? normalizeUuid(record.uuid) : null;
    if (uuid === null) {
      return null;
    }
  }
  return { id: record.id, kind: "offline", name: record.name, uuidMode: record.uuidMode, uuid };
}

/**
 * 把存储里的内容还原成账户簿。损坏的条目被丢弃，重复 ID 只保留第一个，
 * 选中的账户不存在时回落到第一个——存储里的内容不被信任。
 */
export function parseAccountBook(raw: string | null): AccountBook {
  if (raw === null) {
    return emptyAccountBook;
  }
  let data: unknown;
  try {
    data = JSON.parse(raw);
  } catch {
    return emptyAccountBook;
  }
  if (typeof data !== "object" || data === null) {
    return emptyAccountBook;
  }
  const record = data as Record<string, unknown>;
  const seen = new Set<string>();
  const accounts: Account[] = [];
  if (Array.isArray(record.accounts)) {
    for (const item of record.accounts) {
      const account = parseAccount(item);
      if (account !== null && !seen.has(account.id)) {
        seen.add(account.id);
        accounts.push(account);
      }
    }
  }
  const activeId =
    typeof record.activeId === "string" && seen.has(record.activeId)
      ? record.activeId
      : (accounts[0]?.id ?? null);
  return { accounts, activeId };
}

export function loadAccountBook(): AccountBook {
  try {
    return parseAccountBook(window.localStorage.getItem(accountBookStorageKey));
  } catch {
    return emptyAccountBook;
  }
}

export function saveAccountBook(book: AccountBook): void {
  try {
    window.localStorage.setItem(accountBookStorageKey, JSON.stringify(book));
  } catch {
    // 存储不可用时只在本次运行内生效
  }
}
