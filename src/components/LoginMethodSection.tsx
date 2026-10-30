import { Radio, RadioGroup, Text } from "@fluentui/react-components";
import { useState } from "react";
import { isLoginMethod, loadLoginMethod, saveLoginMethod } from "../lib/loginMethod";
import type { LoginMethod } from "../lib/loginMethod";

/**
 * 「账户登录」设置区块：微软登录用设备码还是浏览器授权。
 *
 * 选择只保存在本机，下次点「登录微软账户」时生效。两种方式拿到的都是同一份登录，
 * 令牌同样只留在系统凭据库里；区别只在第一步怎么向微软证明是你本人。
 */
export function LoginMethodSection() {
  const [method, setMethod] = useState<LoginMethod>(loadLoginMethod);

  const change = (next: LoginMethod) => {
    setMethod(next);
    saveLoginMethod(next);
  };

  return (
    <section className="status-panel setting-group" aria-labelledby="login-method-heading">
      <Text id="login-method-heading" as="h2" size={400} weight="semibold">
        账户登录
      </Text>
      <div className="setting-row">
        <div className="setting-copy">
          <Text weight="semibold">微软登录方式</Text>
          <Text size={200} className="muted">
            两种方式登录的是同一个账户，令牌都只保存在系统凭据库里。下次登录时生效。
          </Text>
        </div>
        <div className="setting-control">
          <RadioGroup
            aria-label="微软登录方式"
            value={method}
            onChange={(_, data) => {
              if (isLoginMethod(data.value)) {
                change(data.value);
              }
            }}
          >
            <Radio value="device-code" label="设备码（默认）" />
            <Radio value="browser" label="浏览器授权" />
          </RadioGroup>
        </div>
      </div>
      <Text size={200} className="muted" role="note">
        {method === "device-code"
          ? "启动器给出一个短码，你在任何浏览器（甚至另一台设备）里输入。不要求本机的浏览器能连回启动器。"
          : "启动器直接打开微软登录页，登录后浏览器跳回本机的一个临时端口，不用抄码。只在这台电脑上监听回环地址，登录结束就关闭。需要本机有可用的浏览器。"}
      </Text>
    </section>
  );
}
