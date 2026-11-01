import { Button, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, Field, Input, Radio, RadioGroup, Text } from "@fluentui/react-components";
import { useEffect, useRef, useState } from "react";
import type { FormEvent } from "react";
import { beginYggdrasilLogin, cancelYggdrasilLogin, describeYggdrasilError, finishYggdrasilLogin, isYggdrasilServer, yggdrasilPresets } from "../ipc";
import type { YggdrasilAccount, YggdrasilLogin } from "../ipc";

export function YggdrasilLoginDialog({ onClose, onAdded }: {
  onClose: () => void;
  onAdded: (account: YggdrasilAccount) => void;
}) {
  const [preset, setPreset] = useState("LittleSkin");
  const [server, setServer] = useState<string>(yggdrasilPresets[0].server);
  const [username, setUsername] = useState("");
  // Keep the password in the transient input only, never in account/application state.
  const password = useRef<HTMLInputElement>(null);
  const [login, setLogin] = useState<YggdrasilLogin | null>(null);
  const pendingId = useRef<string | null>(null);
  const mounted = useRef(true);
  const inFlight = useRef(false);
  const [profileId, setProfileId] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      if (pendingId.current) void cancelYggdrasilLogin(pendingId.current).catch(() => undefined);
    };
  }, []);

  const close = () => {
    if (password.current) password.current.value = "";
    onClose();
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (inFlight.current) return;
    setError(null);
    if (!login && (!isYggdrasilServer(server.trim()) || !username.trim() || !password.current?.value)) {
      if (password.current) password.current.value = "";
      setError(describeYggdrasilError(!isYggdrasilServer(server.trim()) ? "yggdrasil-invalid-server" : "yggdrasil-invalid-input"));
      return;
    }
    inFlight.current = true;
    setBusy(true);
    try {
      if (login) {
        const account = await finishYggdrasilLogin(login.id, profileId);
        pendingId.current = null;
        if (mounted.current) { onAdded(account); close(); }
      } else {
        const request = beginYggdrasilLogin(server.trim(), username.trim(), password.current!.value);
        password.current!.value = "";
        const result = await request;
        if (!mounted.current) {
          await cancelYggdrasilLogin(result.id);
          return;
        }
        pendingId.current = result.id;
        if (!result.profiles.length) throw "yggdrasil-no-profile";
        setLogin(result);
        setProfileId(result.profiles.length === 1 ? result.profiles[0].id : "");
      }
    } catch (failure) {
      if (mounted.current) setError(describeYggdrasilError(failure));
    } finally {
      if (password.current) password.current.value = "";
      inFlight.current = false;
      if (mounted.current) setBusy(false);
    }
  };

  return (
    <Dialog open onOpenChange={(_, data) => { if (!data.open && !busy) close(); }}>
      <DialogSurface aria-describedby={undefined}>
        <form onSubmit={(event) => void submit(event)}>
          <DialogBody>
            <DialogTitle>{login ? "选择角色" : "添加第三方账户"}</DialogTitle>
            <DialogContent className="dialog-content">
              <Text className="muted">登录后将使用所选角色启动游戏，凭据仅保存在系统凭据库。</Text>
              {login ? (
                <Field label="角色" required>
                  <RadioGroup value={profileId} onChange={(_, data) => setProfileId(data.value)} disabled={busy}>
                    {login.profiles.map((profile) => <Radio key={profile.id} value={profile.id} label={`${profile.name} · ${profile.id}`} />)}
                  </RadioGroup>
                </Field>
              ) : (<>
                <Field label="认证服务">
                  <RadioGroup layout="horizontal" value={preset} disabled={busy} onChange={(_, data) => {
                    setPreset(data.value);
                    setServer(yggdrasilPresets.find((item) => item.name === data.value)?.server ?? "");
                    if (password.current) password.current.value = "";
                  }}>
                    {yggdrasilPresets.map((item) => <Radio key={item.name} value={item.name} label={item.name} />)}
                    <Radio value="custom" label="自定义" />
                  </RadioGroup>
                </Field>
                <Field label="HTTPS API 根地址" required hint="请确认这是你信任的认证服务；登录信息将发送给此服务。">
                  <Input value={server} readOnly={preset !== "custom"} disabled={busy} onChange={(_, data) => setServer(data.value)} maxLength={2048} />
                </Field>
                <Field label="用户名或邮箱" required><Input value={username} disabled={busy} autoComplete="off" maxLength={320} onChange={(_, data) => setUsername(data.value)} /></Field>
                <Field label="密码" required hint="仅用于本次验证，提交后立即清空。"><Input ref={password} type="password" disabled={busy} autoComplete="off" maxLength={4096} /></Field>
              </>)}
              {busy ? <Text role="status">{login ? "正在保存账户…" : "正在验证账户…"}</Text> : null}
              {error ? <Text role="alert">{error}</Text> : null}
            </DialogContent>
            <DialogActions>
              <Button type="button" onClick={close} disabled={busy}>取消</Button>
              <Button type="submit" appearance="primary" disabled={busy || (login !== null && !profileId)}>{login ? "添加账户" : "验证账户"}</Button>
            </DialogActions>
          </DialogBody>
        </form>
      </DialogSurface>
    </Dialog>
  );
}
