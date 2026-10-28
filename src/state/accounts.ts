import { createContext, useContext } from "react";
import type { AccountBook, OfflineAccount } from "../lib/accounts";

export type AccountsApi = {
  book: AccountBook;
  /** 当前用来启动游戏的账户；没有账户时为 null */
  active: OfflineAccount | null;
  add: (account: OfflineAccount) => void;
  remove: (id: string) => void;
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
