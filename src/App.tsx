import { FluentProvider } from "@fluentui/react-components";
import { useCallback, useEffect, useMemo, useState } from "react";
import { PageTransition } from "./components/PageTransition";
import { defaultApplicationInfo, getApplicationInfo, isTauri, setWindowTheme } from "./ipc";
import type { ApplicationInfo } from "./ipc";
import { aetherDarkTheme, aetherLightTheme } from "./lib/brand";
import { currentGreeting } from "./lib/greeting";
import { loadMotionPreference, saveMotionPreference } from "./lib/motion";
import type { MotionPreference } from "./lib/motion";
import { detectMica } from "./lib/platform";
import { useTheme } from "./lib/useTheme";
import { findPage } from "./navigation";
import type { PageId } from "./navigation";
import { AccountsPage } from "./pages/AccountsPage";
import { DownloadPage } from "./pages/DownloadPage";
import { HomePage } from "./pages/HomePage";
import { InstancesPage } from "./pages/InstancesPage";
import { SettingsPage } from "./pages/SettingsPage";
import { NavPane } from "./shell/NavPane";
import { TitleBar } from "./shell/TitleBar";
import { AccountsProvider } from "./state/AccountsProvider";
import { GameProvider } from "./state/GameProvider";
import { InstallProvider } from "./state/InstallProvider";
import { InstancesProvider } from "./state/InstancesProvider";
import { NavigationContext } from "./state/navigation";
import type { NavigationApi } from "./state/navigation";

function App() {
  const [application, setApplication] = useState<ApplicationInfo>(defaultApplicationInfo);
  const [pageId, setPageId] = useState<PageId>("home");
  const [motion, setMotion] = useState<MotionPreference>(loadMotionPreference);
  const [mica, setMica] = useState(false);
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

  // 窗口材质：Windows 11 上透出 Mica，其他系统铺实色
  useEffect(() => {
    let cancelled = false;
    void detectMica().then((supported) => {
      if (!cancelled) {
        setMica(supported);
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    const root = document.documentElement;
    root.dataset.material = mica ? "mica" : "solid";
    root.dataset.motion = motion;
    root.dataset.theme = resolved;
  }, [mica, motion, resolved]);

  // 让 Mica 的明暗跟随界面，而不是系统
  useEffect(() => {
    void setWindowTheme(resolved).catch(() => undefined);
  }, [resolved]);

  const changeMotion = useCallback((next: MotionPreference) => {
    setMotion(next);
    saveMotionPreference(next);
  }, []);

  const navigation = useMemo<NavigationApi>(
    () => ({ current: pageId, navigate: setPageId }),
    [pageId],
  );

  const page = findPage(pageId);

  return (
    <FluentProvider theme={resolved === "dark" ? aetherDarkTheme : aetherLightTheme} className="app-root">
      <NavigationContext.Provider value={navigation}>
        <AccountsProvider>
          <InstancesProvider>
            <InstallProvider>
              <GameProvider>
                <div className="app-shell">
                  <TitleBar />
                  <div className="app-body">
                    <NavPane version={application.version} />
                    <main className="content-layer">
                      <PageTransition pageKey={pageId}>
                        {page.id === "home" ? (
                          <HomePage greeting={greeting} summary={page.summary} />
                        ) : page.id === "instances" ? (
                          <InstancesPage page={page} />
                        ) : page.id === "download" ? (
                          <DownloadPage page={page} />
                        ) : page.id === "accounts" ? (
                          <AccountsPage page={page} />
                        ) : (
                          <SettingsPage
                            page={page}
                            themePreference={preference}
                            onThemeChange={setPreference}
                            motion={motion}
                            onMotionChange={changeMotion}
                            version={application.version}
                          />
                        )}
                      </PageTransition>
                    </main>
                  </div>
                </div>
              </GameProvider>
            </InstallProvider>
          </InstancesProvider>
        </AccountsProvider>
      </NavigationContext.Provider>
    </FluentProvider>
  );
}

export default App;
