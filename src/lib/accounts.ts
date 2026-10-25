/**
 * 账户页的纯逻辑：玩家名校验与离线账户构造。
 *
 * 规则与 Java 版用户名一致：3–16 位，只允许字母、数字和下划线。
 * 离线账户只存在前端状态里，不写盘、不联网，也不会产生任何令牌。
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
    return { ok: false, error: "请填写玩家名" };
  }
  if (name.length < minPlayerNameLength) {
    return {
      ok: false,
      error: `玩家名至少 ${minPlayerNameLength} 个字符`,
    };
  }
  if (name.length > maxPlayerNameLength) {
    return {
      ok: false,
      error: `玩家名最多 ${maxPlayerNameLength} 个字符`,
    };
  }
  if (!allowedPattern.test(name)) {
    return { ok: false, error: "只能使用字母、数字和下划线" };
  }
  return { ok: true, name };
}

/** 离线账户。只有展示信息——没有 token，也没有密码字段。 */
export type OfflineAccount = {
  id: string;
  name: string;
  kind: "offline";
};

/**
 * 生成离线账户 ID。
 *
 * 用递增序号而不是随机 UUID：离线账户只在本机会话里存在，序号足够区分，
 * 而且测试可以预期。`existing` 传入当前列表即可避免重复。
 */
export function nextOfflineAccountId(
  existing: readonly OfflineAccount[],
): string {
  const used = new Set(existing.map((account) => account.id));
  let index = existing.length + 1;
  while (used.has(`offline-${index}`)) {
    index += 1;
  }
  return `offline-${index}`;
}

/** 添加一个离线账户；名字不合法时返回错误，不改动原列表。 */
export function addOfflineAccount(
  accounts: readonly OfflineAccount[],
  rawName: string,
): { ok: true; accounts: OfflineAccount[] } | { ok: false; error: string } {
  const validation = validatePlayerName(rawName);
  if (!validation.ok) {
    return { ok: false, error: validation.error };
  }

  const duplicate = accounts.some(
    (account) => account.name.toLowerCase() === validation.name.toLowerCase(),
  );
  if (duplicate) {
    return { ok: false, error: "已经有同名账户了" };
  }

  const account: OfflineAccount = {
    id: nextOfflineAccountId(accounts),
    name: validation.name,
    kind: "offline",
  };

  return { ok: true, accounts: [...accounts, account] };
}

/** 移除一个离线账户。 */
export function removeOfflineAccount(
  accounts: readonly OfflineAccount[],
  id: string,
): OfflineAccount[] {
  return accounts.filter((account) => account.id !== id);
}

/** 列表摘要，例如 `2 个离线账户`。 */
export function summariseOfflineAccounts(
  accounts: readonly OfflineAccount[],
): string {
  if (accounts.length === 0) {
    return "还没有离线账户";
  }
  return `${accounts.length} 个离线账户`;
}
