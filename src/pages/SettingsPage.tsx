import { Radio, RadioGroup, Text } from "@fluentui/react-components";
import { JavaRuntimeSection } from "../components/JavaRuntimeSection";
import { MemorySection } from "../components/MemorySection";
import { PageHeader } from "../components/PageHeader";
import { isThemePreference } from "../lib/theme";
import type { ThemePreference } from "../lib/theme";
import type { PageDefinition } from "../navigation";

type SettingsPageProps = {
  page: PageDefinition;
  themePreference: ThemePreference;
  onThemeChange: (preference: ThemePreference) => void;
};

export function SettingsPage({
  page,
  themePreference,
  onThemeChange,
}: SettingsPageProps) {
  return (
    <>
      <PageHeader title={page.label} description={page.summary} />

      <section
        className="status-panel settings-panel"
        aria-labelledby="appearance-heading"
      >
        <Text id="appearance-heading" as="h2" size={400} weight="semibold">
          外观
        </Text>
        <Text className="muted">选择启动器的配色方案，选择会保存在本机</Text>
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
      </section>

      <JavaRuntimeSection />
      <MemorySection />
    </>
  );
}
