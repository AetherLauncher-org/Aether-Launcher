import { Radio, RadioGroup, Switch, Text } from "@fluentui/react-components";
import type { ReactNode } from "react";
import { JavaRuntimeSection } from "../components/JavaRuntimeSection";
import { LoginMethodSection } from "../components/LoginMethodSection";
import { MemorySection } from "../components/MemorySection";
import { PageHeader } from "../components/PageHeader";
import { isThemePreference } from "../lib/theme";
import type { ThemePreference } from "../lib/theme";
import type { MotionPreference } from "../lib/motion";
import type { PageDefinition } from "../navigation";

type SettingsPageProps = {
  page: PageDefinition;
  themePreference: ThemePreference;
  onThemeChange: (preference: ThemePreference) => void;
  motion: MotionPreference;
  onMotionChange: (preference: MotionPreference) => void;
  version: string;
};

type SettingRowProps = {
  title: string;
  description: string;
  children: ReactNode;
};

/** 一行设置：左边是名称和说明，右边是控件，沿用 Windows 设置的排法。 */
function SettingRow({ title, description, children }: SettingRowProps) {
  return (
    <div className="setting-row">
      <div className="setting-copy">
        <Text weight="semibold">{title}</Text>
        <Text size={200} className="muted">
          {description}
        </Text>
      </div>
      <div className="setting-control">{children}</div>
    </div>
  );
}

export function SettingsPage({
  page,
  themePreference,
  onThemeChange,
  motion,
  onMotionChange,
  version,
}: SettingsPageProps) {
  return (
    <>
      <PageHeader title={page.label} description={page.summary} />

      <section className="status-panel setting-group" aria-labelledby="appearance-heading">
        <Text id="appearance-heading" as="h2" size={400} weight="semibold">
          外观
        </Text>
        <SettingRow title="配色方案" description="选择启动器的明暗，选择会保存在本机">
          <RadioGroup
            aria-label="配色方案"
            layout="horizontal"
            value={themePreference}
            onChange={(_, data) => {
              if (isThemePreference(data.value)) {
                onThemeChange(data.value);
              }
            }}
          >
            <Radio value="dark" label="深色" />
            <Radio value="light" label="浅色" />
            <Radio value="system" label="跟随系统" />
          </RadioGroup>
        </SettingRow>
        <SettingRow title="界面动画" description="页面切换、导航栏收起和列表出现时的过渡效果">
          <Switch
            aria-label="界面动画"
            checked={motion === "full"}
            onChange={(_, data) => onMotionChange(data.checked ? "full" : "reduced")}
            label={motion === "full" ? "开" : "关"}
          />
        </SettingRow>
      </section>

      <MemorySection />
      <JavaRuntimeSection />
      <LoginMethodSection />

      <section className="status-panel setting-group" aria-labelledby="download-heading">
        <Text id="download-heading" as="h2" size={400} weight="semibold">
          下载来源
        </Text>
        <SettingRow
          title="Mojang 官方"
          description="版本清单、游戏文件、依赖库和资源都只从 Mojang 官方服务器下载，每个文件的 SHA-1 取自官方清单。"
        >
          <Text className="muted">唯一信任来源</Text>
        </SettingRow>
      </section>

      <section className="status-panel setting-group" aria-labelledby="about-heading">
        <Text id="about-heading" as="h2" size={400} weight="semibold">
          关于
        </Text>
        <SettingRow title="Aether Launcher" description="开放、透明、可恢复的 Minecraft 实例管理器。Apache-2.0 许可。">
          <Text className="muted">版本 {version}</Text>
        </SettingRow>
      </section>
    </>
  );
}
