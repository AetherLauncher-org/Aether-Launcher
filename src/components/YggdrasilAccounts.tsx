import { Badge, Button, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, Text } from "@fluentui/react-components";
import { useState } from "react";
import { describeYggdrasilError, listYggdrasilAccounts, refreshYggdrasilAccount, removeYggdrasilAccount } from "../ipc";
import type { YggdrasilAccount } from "../ipc";
import type { YggdrasilAccountEntry } from "../lib/accounts";
import { useAccounts } from "../state/accounts";
import { YggdrasilLoginDialog } from "./YggdrasilLoginDialog";

function entry(account: YggdrasilAccount): YggdrasilAccountEntry {
  return { id: account.id, kind: "yggdrasil", name: account.profile.name, uuid: account.profile.id, server: account.server };
}

export function YggdrasilAccounts({ adding, onClose }: { adding: boolean; onClose: () => void }) {
  const { book, active, select, remove, signInYggdrasil, syncYggdrasil } = useAccounts();
  const accounts = book.accounts.filter((account): account is YggdrasilAccountEntry => account.kind === "yggdrasil");
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [removing, setRemoving] = useState<YggdrasilAccountEntry | null>(null);

  const reload = async () => {
    try {
      const listed = await listYggdrasilAccounts();
      if (listed === null) {
        setError(describeYggdrasilError("yggdrasil-network-error"));
        return;
      }
      syncYggdrasil(listed.map(entry));
      setError(null);
    } catch (failure) { setError(describeYggdrasilError(failure)); }
  };
  const act = async (account: YggdrasilAccountEntry, deleting: boolean) => {
    if (busy) return;
    setBusy(true); setError(null); setStatus(null);
    try {
      if (deleting) {
        await removeYggdrasilAccount(account.id);
        remove(account.id);
        setRemoving(null);
      } else {
        await refreshYggdrasilAccount(account.id);
        await reload();
        setStatus(`已刷新 ${account.name} 的登录状态。`);
      }
    } catch (failure) { setError(describeYggdrasilError(failure)); }
    finally { setBusy(false); }
  };

  return (
    <section className="section" data-account-group="thirdparty" aria-labelledby="thirdparty-heading">
      <div className="section-heading">
        <Text id="thirdparty-heading" as="h2" size={500} weight="semibold">第三方账户</Text>
        <Text className="muted">{accounts.length} 个</Text>
        <Button size="small" disabled={busy} onClick={() => void reload()}>重新加载</Button>
      </div>
      {accounts.length === 0 ? <Text className="muted account-group-empty">还没有第三方账户。添加 LittleSkin、Ely.by 或自定义认证服务的账户。</Text> : (
        <ul className="account-grid">
          {accounts.map((account) => <li key={account.id} className={`account-card${active?.id === account.id ? " active" : ""}`}>
            <button type="button" className="account-card-main" aria-label={`使用 ${account.name}`} aria-pressed={active?.id === account.id} onClick={() => select(account.id)}>
              <div className="account-card-body">
                <Text weight="semibold" size={400}>{account.name}</Text>
                {active?.id === account.id ? <Badge appearance="tint" color="brand" size="small">当前使用</Badge> : null}
                <Text size={200} className="muted" style={{ overflowWrap: "anywhere" }}>{account.server}</Text>
                <Text size={200} className="muted mono">{account.uuid}</Text>
              </div>
            </button>
            <div className="account-card-actions">
              <Button size="small" disabled={busy} onClick={() => void act(account, false)} aria-label={`刷新 ${account.name}`}>刷新</Button>
              <Button size="small" disabled={busy} onClick={() => { setError(null); setRemoving(account); }} aria-label={`移除 ${account.name}`}>移除</Button>
            </div>
          </li>)}
        </ul>
      )}
      {status ? <Text role="status">{status}</Text> : null}
      {error && !removing ? <Text role="alert">{error}</Text> : null}
      {adding ? <YggdrasilLoginDialog onClose={onClose} onAdded={(account) => {
        signInYggdrasil(entry(account));
        setError(null);
      }} /> : null}
      <Dialog open={removing !== null} onOpenChange={(_, data) => { if (!data.open && !busy) setRemoving(null); }}>
        <DialogSurface aria-describedby={undefined}><DialogBody>
          <DialogTitle>移除第三方账户「{removing?.name}」？</DialogTitle>
          <DialogContent className="dialog-content">
            <Text>将删除这台电脑上保存的账户和凭据，之后可以重新添加。</Text>
            {error ? <Text role="alert">{error}</Text> : null}
          </DialogContent>
          <DialogActions>
            <Button disabled={busy} onClick={() => setRemoving(null)}>取消</Button>
            <Button appearance="primary" disabled={busy} onClick={() => { if (removing) void act(removing, true); }}>移除</Button>
          </DialogActions>
        </DialogBody></DialogSurface>
      </Dialog>
    </section>
  );
}
