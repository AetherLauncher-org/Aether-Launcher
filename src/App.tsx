import {
  FluentProvider,
  webDarkTheme,
  webLightTheme,
} from "@fluentui/react-components";
import { useEffect, useState } from "react";
import "./App.css";
import { Sidebar } from "./components/Sidebar";
import {
  defaultApplicationInfo,
  getApplicationInfo,
  isTauri,
} from "./ipc";
import type { ApplicationInfo } from "./ipc";
import { currentGreeting } from "./lib/greeting";
import { useTheme } from "./lib/useTheme";
import { allPages } from "./navigation";
import type { PageId } from "./navigation";
import { HomePage } from "./pages/HomePage";
import { AccountsPage } from "./pages/AccountsPage";
import { InstancesPage } from "./pages/InstancesPage";
import { LibraryPage } from "./pages/LibraryPage";
import { PlaceholderPage } from "./pages/PlaceholderPage";
import { SettingsPage } from "./pages/SettingsPage";

function App() {
  const [application, setApplication] =
    useState<ApplicationInfo>(defaultApplicationInfo);
  const [pageId, setPageId] = useState<PageId>("home");
  const [greeting] = useState(currentGreeting);
  const { preference, resolved, setPreference } = useTheme();

  useEffect(() => {
    if (!isTauri()) {
      return;
    }

    let cancelled = false;
    void getApplicationInfo()
      .then((info) => {
        if (!cancelled) {
          setApplication(info);
        }
      })
      .catch(() => undefined);

    return () => {
      cancelled = true;
    };
  }, []);

  const page = allPages.find((candidate) => candidate.id === pageId) ?? allPages[0];

  return (
    <FluentProvider theme={resolved === "dark" ? webDarkTheme : webLightTheme}>
      <div className="app-shell" data-theme={resolved}>
        <Sidebar
          application={application}
          current={pageId}
          onNavigate={setPageId}
        />
        <main className="workspace">
          {page.id === "home" ? (
            <HomePage
              application={application}
              greeting={greeting}
              summary={page.summary}
            />
          ) : page.id === "settings" ? (
            <SettingsPage
              page={page}
              themePreference={preference}
              onThemeChange={setPreference}
            />
          ) : page.id === "instances" ? (
            <InstancesPage page={page} />
          ) : page.id === "library" ? (
            <LibraryPage page={page} />
          ) : page.id === "accounts" ? (
            <AccountsPage page={page} />
          ) : (
            <PlaceholderPage page={page} />
          )}
        </main>
      </div>
    </FluentProvider>
  );
}

export default App;
