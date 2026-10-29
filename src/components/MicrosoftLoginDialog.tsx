import {
  Button,
  Dialog,
  DialogActions,
  DialogBody,
  DialogContent,
  DialogSurface,
  DialogTitle,
  MessageBar,
  MessageBarBody,
  Spinner,
  Text,
} from "@fluentui/react-components";
import { Copy20Regular, Open20Regular } from "@fluentui/react-icons";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  beginMicrosoftLogin,
  cancelMicrosoftLogin,
  describeCommandError,
  describeLoginFailure,
  describeMicrosoftError,
  listenLoginStatus,
  openMicrosoftVerificationPage,
} from "../ipc";
import type { DeviceCodeInfo, MicrosoftAccount } from "../ipc";

type MicrosoftLoginDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** 登录成功后回调；账户只带身份，没有令牌 */
  onSignedIn: (account: MicrosoftAccount) => void;
};

type Phase =
  | { kind: "starting" }
  | { kind: "waiting"; code: DeviceCodeInfo }
  | { kind: "done"; account: MicrosoftAccount }
  | { kind: "failed"; message: string };

/**
 * 微软账户登录对话框（设备码流程）。
 *
 * 打开时向后端申请一个短码，显示给用户；用户在浏览器里输入后，后端在后台完成登录，
 * 状态通过 `login://status` 事件回来。关闭对话框会取消还没完成的登录。
 * 这里看不到任何令牌：只有设备码的展示值和登录出来的账户身份。
 */
export function MicrosoftLoginDialog({ open, onOpenChange, onSignedIn }: MicrosoftLoginDialogProps) {
  const [phase, setPhase] = useState<Phase>({ kind: "starting" });
  const [attempt, setAttempt] = useState(0);
  const [notice, setNotice] = useState<string | null>(null);
  const onSignedInRef = useRef(onSignedIn);

  useEffect(() => {
    onSignedInRef.current = onSignedIn;
  }, [onSignedIn]);

  useEffect(() => {
    if (!open) {
      return;
    }
    let active = true;
    let unlisten: (() => void) | undefined;

    void (async () => {
      try {
        unlisten = await listenLoginStatus((event) => {
          if (!active) {
            return;
          }
          if (event.state === "signed-in") {
            setPhase({ kind: "done", account: event.account });
            onSignedInRef.current(event.account);
          } else if (event.state === "failed") {
            setPhase({ kind: "failed", message: describeLoginFailure(event.failure) });
          }
        });
        const code = await beginMicrosoftLogin();
        if (active) {
          setPhase({ kind: "waiting", code });
        }
      } catch (failure: unknown) {
        if (active) {
          setPhase({
            kind: "failed",
            message: describeMicrosoftError(describeCommandError(failure)),
          });
        }
      }
    })();

    return () => {
      active = false;
      unlisten?.();
      // 没有登录在进行时这是空操作
      void cancelMicrosoftLogin();
    };
  }, [open, attempt]);

  const copy = useCallback(async (text: string, what: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setNotice(`已复制${what}`);
    } catch {
      setNotice(`无法复制，请手动选中${what}`);
    }
  }, []);

  const openPage = useCallback(async () => {
    try {
      await openMicrosoftVerificationPage();
      setNotice(null);
    } catch {
      setNotice("无法自动打开浏览器，请复制下面的网址手动打开");
    }
  }, []);

  // 每次关闭都回到初始状态，下次打开时是干净的；重试也是同样的重置
  const reset = useCallback(() => {
    setPhase({ kind: "starting" });
    setNotice(null);
  }, []);

  const handleOpenChange = (next: boolean) => {
    if (!next) {
      reset();
    }
    onOpenChange(next);
  };

  return (
    <Dialog open={open} onOpenChange={(_, data) => handleOpenChange(data.open)}>
      <DialogSurface aria-describedby={undefined}>
        <DialogBody>
          <DialogTitle>登录微软账户</DialogTitle>
          <DialogContent className="dialog-content">
            {phase.kind === "starting" ? (
              <Spinner size="tiny" label="正在向微软申请登录码…" />
            ) : null}

            {phase.kind === "waiting" ? (
              <>
                <ol className="login-steps">
                  <li>
                    <div className="login-step">
                    <Text>在浏览器里打开微软的登录页面。</Text>
                    <div className="login-step-actions">
                      <Button icon={<Open20Regular />} onClick={() => void openPage()}>
                        打开登录页面
                      </Button>
                      <Button
                        appearance="subtle"
                        icon={<Copy20Regular />}
                        onClick={() => void copy(phase.code.verificationUri, "网址")}
                      >
                        复制网址
                      </Button>
                    </div>
                    <Text size={200} className="muted mono">
                      {phase.code.verificationUri}
                    </Text>
                    </div>
                  </li>
                  <li>
                    <div className="login-step">
                    <Text>输入这个登录码：</Text>
                    <div className="login-step-actions">
                      <span className="device-code" aria-label="登录码">
                        {phase.code.userCode}
                      </span>
                      <Button
                        appearance="subtle"
                        icon={<Copy20Regular />}
                        onClick={() => void copy(phase.code.userCode, "登录码")}
                      >
                        复制登录码
                      </Button>
                    </div>
                    </div>
                  </li>
                  <li>
                    <div className="login-step">
                    <Text>用拥有 Minecraft 的微软账户登录并同意，这里会自动继续。</Text>
                    </div>
                  </li>
                </ol>
                <Spinner size="tiny" label="等待你在浏览器里完成登录…" />
                <Text size={200} className="muted">
                  登录码 {Math.max(1, Math.round(phase.code.expiresInSecs / 60))} 分钟内有效。
                  令牌只保存在本机的系统凭据库里，不会显示在这里。
                </Text>
              </>
            ) : null}

            {phase.kind === "done" ? (
              <MessageBar intent="success" role="status">
                <MessageBarBody>已登录：{phase.account.displayName}</MessageBarBody>
              </MessageBar>
            ) : null}

            {phase.kind === "failed" ? (
              <MessageBar intent="error" role="alert">
                <MessageBarBody>{phase.message}</MessageBarBody>
              </MessageBar>
            ) : null}

            {notice ? (
              <Text size={200} role="status" className="muted">
                {notice}
              </Text>
            ) : null}
          </DialogContent>
          <DialogActions>
            {phase.kind === "failed" ? (
              <Button
                appearance="primary"
                onClick={() => {
                  reset();
                  setAttempt((count) => count + 1);
                }}
              >
                重试
              </Button>
            ) : null}
            <Button
              appearance={phase.kind === "done" ? "primary" : "secondary"}
              onClick={() => handleOpenChange(false)}
            >
              {phase.kind === "done" ? "完成" : "取消"}
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}
