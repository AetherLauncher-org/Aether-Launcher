import { createContext, useContext } from "react";
import type { Account, AccountBook, MicrosoftAccountEntry, YggdrasilAccountEntry, OfflineAccount } from "../lib/accounts";

export type AccountsApi = {
  book: AccountBook;
  /** 当前用来启动游戏的账户；没有账户时为 null */
  active: Account | null;
  add: (account: OfflineAccount) => void;
  remove: (id: string) => void;
  /** 刚登录了一个微软账户：加进列表并设为当前账户 */
  signInMicrosoft: (account: MicrosoftAccountEntry) => void;
  signInYggdrasil: (account: YggdrasilAccountEntry) => void;
  syncYggdrasil: (accounts: YggdrasilAccountEntry[]) => void;
  select: (id: string) => void;
};

export const AccountsContext = createContext<AccountsApi | null>(null);

export function useAccounts(): AccountsApi {
  const value = useContext(AccountsContext);
  if (value === null) {
    throw new Error("useAccounts 需要在 AccountsProvider 内使用");
  }
  return value;
}
