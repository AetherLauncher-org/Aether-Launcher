import { Badge, Button, Text, Tooltip } from "@fluentui/react-components";
import { Play20Filled, Stop20Regular } from "@fluentui/react-icons";
import type { InstanceSummary } from "../ipc";
import { canStart, canStop, describeSession, startLabel } from "../lib/game";
import { describeInstanceState } from "../lib/instances";
import { useAccounts } from "../state/accounts";
import { useGame } from "../state/game";
import { useNavigation } from "../state/navigation";

type GameControlsProps = {
  instance: InstanceSummary;
  size?: "small" | "medium" | "large";
  /** 不显示状态徽章（调用方自己排版时用） */
  hideBadge?: boolean;
};

/** 启动／停止按钮加状态徽章。实例文件不完整或没有账户时给出明确的下一步，而不是一个灰按钮。 */
export function GameControls({ instance, size = "medium", hideBadge = false }: GameControlsProps) {
  const { sessionOf, start, stop } = useGame();
  const { active } = useAccounts();
  const { navigate } = useNavigation();

  const session = sessionOf(instance.id);
  const ready = instance.state === "ready";
  // 空闲时徽章反映实例记录本身的状态（例如「待补齐」），而不是笼统的「就绪」
  const view =
    session.phase === "idle" && !ready
      ? describeInstanceState(instance.state)
      : describeSession(session);

  let button;
  if (canStop(session)) {
    button = (
      <Button
        size={size}
        appearance="secondary"
        icon={<Stop20Regular />}
        onClick={() => void stop(instance.id)}
      >
        停止
      </Button>
    );
  } else if (!ready) {
    button = (
      <Tooltip content="文件不完整，请到「下载」页重新安装这个版本" relationship="description">
        <Button size={size} appearance="primary" disabledFocusable icon={<Play20Filled />}>
          启动
        </Button>
      </Tooltip>
    );
  } else if (active === null) {
    button = (
      <Button size={size} appearance="primary" onClick={() => navigate("accounts")}>
        先添加账户
      </Button>
    );
  } else {
    button = (
      <Button
        size={size}
        appearance="primary"
        icon={<Play20Filled />}
        disabled={!canStart(session)}
        onClick={() => void start(instance)}
      >
        {startLabel(session)}
      </Button>
    );
  }

  return (
    <div className="game-controls">
      <div className="game-controls-row">
        {hideBadge ? null : (
          <Badge appearance="tint" color={view.tone === "subtle" ? "subtle" : view.tone}>
            {view.label}
          </Badge>
        )}
        {button}
      </div>
      {session.error ? (
        <Text role="alert" size={200} className="game-error">
          {session.error}
        </Text>
      ) : null}
    </div>
  );
}
