import { useCallback, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { listMicrosoftAccounts } from "../ipc";
import {
  activeAccount,
  addAccount,
  loadAccountBook,
  removeAccount,
  saveAccountBook,
  selectAccount,
  signInMicrosoftAccount,
  syncMicrosoftAccounts,
} from "../lib/accounts";
import type { Account, AccountBook, MicrosoftAccountEntry } from "../lib/accounts";
import { AccountsContext } from "./accounts";
import type { AccountsApi } from "./accounts";

/**
 * 账户簿放在内存里，每次变更写回 `localStorage`。离线账户没有秘密，可以明文存。
 * 微软账户以后端为准：启动时读一次后端的列表，把账户簿对齐过去。
 */
export function AccountsProvider({ children }: { children: ReactNode }) {
  const [book, setBook] = useState<AccountBook>(loadAccountBook);

  const update = useCallback((change: (previous: AccountBook) => AccountBook) => {
    setBook((previous) => {
      const next = change(previous);
      saveAccountBook(next);
      return next;
    });
  }, []);

  useEffect(() => {
    let current = true;
    void listMicrosoftAccounts().then((accounts) => {
      if (!current || accounts === null) {
        return;
      }
      const entries: MicrosoftAccountEntry[] = accounts.map((account) => ({
        id: account.id,
        kind: "microsoft",
        name: account.displayName,
        uuid: account.uuid,
      }));
      update((previous) => syncMicrosoftAccounts(previous, entries));
    });
    return () => {
      current = false;
    };
  }, [update]);

  const api = useMemo<AccountsApi>(
    () => ({
      book,
      active: activeAccount(book),
      add: (account: Account) => update((previous) => addAccount(previous, account)),
      remove: (id: string) => update((previous) => removeAccount(previous, id)),
      select: (id: string) => update((previous) => selectAccount(previous, id)),
      signInMicrosoft: (account: MicrosoftAccountEntry) =>
        update((previous) => signInMicrosoftAccount(previous, account)),
    }),
    [book, update],
  );

  return <AccountsContext.Provider value={api}>{children}</AccountsContext.Provider>;
}
