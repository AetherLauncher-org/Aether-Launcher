import { useCallback, useMemo, useState } from "react";
import type { ReactNode } from "react";
import {
  activeAccount,
  addAccount,
  loadAccountBook,
  removeAccount,
  saveAccountBook,
  selectAccount,
} from "../lib/accounts";
import type { AccountBook, OfflineAccount } from "../lib/accounts";
import { AccountsContext } from "./accounts";
import type { AccountsApi } from "./accounts";

/** 账户簿放在内存里，每次变更写回 `localStorage`。离线账户没有秘密，可以明文存。 */
export function AccountsProvider({ children }: { children: ReactNode }) {
  const [book, setBook] = useState<AccountBook>(loadAccountBook);

  const update = useCallback((change: (previous: AccountBook) => AccountBook) => {
    setBook((previous) => {
      const next = change(previous);
      saveAccountBook(next);
      return next;
    });
  }, []);

  const api = useMemo<AccountsApi>(
    () => ({
      book,
      active: activeAccount(book),
      add: (account: OfflineAccount) => update((previous) => addAccount(previous, account)),
      remove: (id: string) => update((previous) => removeAccount(previous, id)),
      select: (id: string) => update((previous) => selectAccount(previous, id)),
    }),
    [book, update],
  );

  return <AccountsContext.Provider value={api}>{children}</AccountsContext.Provider>;
}
