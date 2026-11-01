import { Badge, Button, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, Text } from "@fluentui/react-components";
import { useEffect, useState } from "react";
import { describeYggdrasilError, isTauri, listYggdrasilAccounts, refreshYggdrasilAccount, removeYggdrasilAccount } from "../ipc";
import type { YggdrasilAccount } from "../ipc";
import { YggdrasilLoginDialog } from "./YggdrasilLoginDialog";

/** Deliberately separate from the launch-selectable account book. */
export function YggdrasilAccounts({ adding, onClose }: { adding: boolean; onClose: () => void }) {
  const [accounts, setAccounts] = useState<YggdrasilAccount[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [removing, setRemoving] = useState<YggdrasilAccount | null>(null);
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    let current = true;
    if (isTauri()) void listYggdrasilAccounts().then((items) => {
      if (current) { setAccounts(items); setError(null); }
    }).catch((failure: unknown) => { if (current) setError(describeYggdrasilError(failure)); });
    return () => { current = false; };
  }, [revision]);

  const act = async (account: YggdrasilAccount, remove: boolean) => {
    if (busy) return;
    setBusy(true); setError(null); setStatus(null);
    try {
      if (remove) {
        await removeYggdrasilAccount(account.id);
        setAccounts((items) => items.filter((item) => item.id !== account.id));
        setRemoving(null);
      } else {
        await refreshYggdrasilAccount(account.id);
        setStatus(`已刷新 ${account.profile.name} 的登录状态。`);
      }
    } catch (failure) { setError(describeYggdrasilError(failure)); }
    finally { setBusy(false); }
  };

  return (
    <section className="section" data-account-group="thirdparty" aria-labelledby="thirdparty-heading">
      <div className="section-heading">
        <Text id="thirdparty-heading" as="h2" size={500} weight="semibold">第三方账户</Text>
        <Badge appearance="outline">暂不能用于启动</Badge>
        <Text className="muted">{accounts.length} 个</Text>
      </div>
      {accounts.length === 0 ? <Text className="muted account-group-empty">还没有第三方账户。添加 LittleSkin、Ely.by 或自定义认证服务的账户。</Text> : (
        <ul className="account-grid">
          {accounts.map((account) => <li key={account.id} className="account-card">
            <div className="account-card-main">
              <div className="account-card-body">
                <Text weight="semibold" size={400}>{account.profile.name}</Text>
                <Text size={200} className="muted" style={{ overflowWrap: "anywhere" }}>{account.server}</Text>
                <Text size={200} className="muted mono">{account.profile.id}</Text>
              </div>
            </div>
            <div className="account-card-actions">
              <Button size="small" disabled={busy} onClick={() => void act(account, false)} aria-label={`刷新 ${account.profile.name}`}>刷新</Button>
              <Button size="small" disabled={busy} onClick={() => { setError(null); setRemoving(account); }} aria-label={`移除 ${account.profile.name}`}>移除</Button>
            </div>
          </li>)}
        </ul>
      )}
      {status ? <Text role="status">{status}</Text> : null}
      {error && !removing ? <div role="alert"><Text>{error}</Text><Button onClick={() => setRevision((value) => value + 1)}>重新加载</Button></div> : null}
      {adding ? <YggdrasilLoginDialog onClose={onClose} onAdded={(account) => {
        setAccounts((items) => [...items.filter((item) => item.id !== account.id), account]);
        setError(null);
      }} /> : null}
      <Dialog open={removing !== null} onOpenChange={(_, data) => { if (!data.open && !busy) setRemoving(null); }}>
        <DialogSurface aria-describedby={undefined}><DialogBody>
          <DialogTitle>移除第三方账户「{removing?.profile.name}」？</DialogTitle>
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
