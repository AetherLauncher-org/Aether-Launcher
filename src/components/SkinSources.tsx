import {
  Badge,
  Button,
  MessageBar,
  MessageBarBody,
  SearchBox,
  Select,
  Spinner,
  Text,
} from "@fluentui/react-components";
import { Add20Regular, ArrowDownload20Regular, Open16Regular, Search20Regular } from "@fluentui/react-icons";
import { useEffect, useRef, useState } from "react";
import {
  addDownloadedSkin,
  addSkinFromFile,
  describeCommandError,
  openSkinSite,
  saveDownloadedSkin,
  searchSkin,
} from "../ipc";
import type { AddSkinResult, SkinPreview, SkinSite, SkinSource } from "../ipc";
import { drawFigure, figureSize } from "../lib/skinFigure";

const sources: readonly { value: SkinSource; label: string }[] = [
  { value: "mojang", label: "Mojang" },
  { value: "littleskin", label: "LittleSkin" },
  { value: "elyby", label: "Ely.by" },
];

const sites: readonly { key: SkinSite; label: string }[] = [
  { key: "namemc", label: "NameMC" },
  { key: "skindex", label: "Skindex" },
  { key: "planetminecraft", label: "Planet Minecraft" },
];

const sourceLabels = Object.fromEntries(sources.map((source) => [source.value, source.label])) as Record<
  SkinSource,
  string
>;

type Busy = "search" | "add" | "save" | "file" | `site:${SkinSite}` | null;
type Notice = { intent: "success" | "info" | "error"; text: string };

function addedNotice(result: AddSkinResult): Notice {
  return result.added
    ? { intent: "success", text: `已加进皮肤库：${result.entry.name}` }
    : { intent: "info", text: `皮肤库里已经有这张皮肤了：${result.entry.name}` };
}

/** Download a skin by player name, then add it to the library or save its PNG. */
export function SkinSources() {
  const [source, setSource] = useState<SkinSource>("mojang");
  const [draft, setDraft] = useState("");
  const [preview, setPreview] = useState<SkinPreview | null>(null);
  const [busy, setBusy] = useState<Busy>(null);
  const [notice, setNotice] = useState<Notice | null>(null);

  const name = draft.trim();
  const validName = /^[A-Za-z0-9_]{3,16}$/.test(name);
  const showInvalid = draft.trim() !== "" && !validName;
  const locked = busy !== null;

  const search = async () => {
    if (!validName || busy !== null) {
      return;
    }
    setBusy("search");
    setNotice(null);
    setPreview(null);
    try {
      setPreview(await searchSkin(source, name));
    } catch (error: unknown) {
      setNotice({ intent: "error", text: `搜索失败：${describeCommandError(error)}` });
    } finally {
      setBusy(null);
    }
  };

  const add = async () => {
    if (preview === null || busy !== null) {
      return;
    }
    setBusy("add");
    setNotice(null);
    try {
      setNotice(addedNotice(await addDownloadedSkin(preview.token)));
    } catch (error: unknown) {
      setNotice({ intent: "error", text: `加入皮肤库失败：${describeCommandError(error)}` });
    } finally {
      setBusy(null);
    }
  };

  const save = async () => {
    if (preview === null || busy !== null) {
      return;
    }
    setBusy("save");
    setNotice(null);
    try {
      const saved = await saveDownloadedSkin(preview.token);
      if (saved !== null) {
        setNotice({ intent: "success", text: `已保存 PNG：${saved.fileName}` });
      }
    } catch (error: unknown) {
      setNotice({ intent: "error", text: `保存皮肤失败：${describeCommandError(error)}` });
    } finally {
      setBusy(null);
    }
  };

  const addFile = async () => {
    if (busy !== null) {
      return;
    }
    setBusy("file");
    setNotice(null);
    try {
      const result = await addSkinFromFile();
      if (result !== null) {
        setNotice(addedNotice(result));
      }
    } catch (error: unknown) {
      setNotice({ intent: "error", text: `添加皮肤文件失败：${describeCommandError(error)}` });
    } finally {
      setBusy(null);
    }
  };

  const openSite = async (site: (typeof sites)[number]) => {
    if (busy !== null) {
      return;
    }
    setBusy(`site:${site.key}`);
    setNotice(null);
    try {
      await openSkinSite(site.key);
    } catch (error: unknown) {
      setNotice({ intent: "error", text: `打开 ${site.label} 失败：${describeCommandError(error)}` });
    } finally {
      setBusy(null);
    }
  };

  return (
    <section className="skin-sources" aria-label="皮肤来源">
      <div className="skin-source-search">
        <div className="skin-source-controls">
          <Select
            aria-label="皮肤来源"
            value={source}
            disabled={locked}
            onChange={(_, data) => {
              setSource(data.value as SkinSource);
              setPreview(null);
              setNotice(null);
            }}
          >
            {sources.map((item) => (
              <option key={item.value} value={item.value}>
                {item.label}
              </option>
            ))}
          </Select>
          <SearchBox
            className="skin-player-search"
            placeholder="输入玩家名"
            aria-label="玩家名"
            value={draft}
            maxLength={32}
            disabled={locked}
            aria-invalid={showInvalid}
            onChange={(_, data) => setDraft(data.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                void search();
              }
            }}
          />
          <Button
            appearance="primary"
            icon={busy === "search" ? <Spinner size="tiny" /> : <Search20Regular />}
            disabled={!validName || locked}
            onClick={() => void search()}
          >
            搜索
          </Button>
        </div>
        <div className="skin-source-file">
          <Text size={200} className="muted">
            已有 PNG 皮肤？
          </Text>
          <Button
            appearance="secondary"
            icon={busy === "file" ? <Spinner size="tiny" /> : <Add20Regular />}
            disabled={locked}
            onClick={() => void addFile()}
          >
            添加皮肤文件
          </Button>
        </div>
      </div>

      {showInvalid ? (
        <Text size={200} className="skin-source-validation">
          玩家名需为 3–16 个字母、数字或下划线。
        </Text>
      ) : null}

      {notice ? (
        <MessageBar intent={notice.intent} role={notice.intent === "error" ? "alert" : "status"}>
          <MessageBarBody>{notice.text}</MessageBarBody>
        </MessageBar>
      ) : null}

      {preview ? (
        <section className="skin-source-preview" aria-label="皮肤搜索结果">
          <div className="skin-source-figure">
            <PreviewFigure key={preview.token} preview={preview} />
          </div>
          <div className="skin-source-result">
            <div className="skin-source-result-title">
              <Text as="h2" size={500} weight="semibold">
                {preview.name}
              </Text>
              <div className="skin-source-badges">
                <Badge appearance="tint" color="informative">
                  {sourceLabels[preview.source]}
                </Badge>
                <Badge appearance="outline" color="subtle">
                  {preview.slim ? "纤细手臂" : "经典手臂"}
                </Badge>
              </div>
            </div>
            <Text size={200} className="muted">
              可加入皮肤库备用，或把原始 PNG 保存到自己选择的文件夹。
            </Text>
            <div className="skin-source-actions">
              <Button
                appearance="primary"
                icon={busy === "add" ? <Spinner size="tiny" /> : <Add20Regular />}
                disabled={locked}
                onClick={() => void add()}
              >
                加入皮肤库
              </Button>
              <Button
                appearance="secondary"
                icon={busy === "save" ? <Spinner size="tiny" /> : <ArrowDownload20Regular />}
                disabled={locked}
                onClick={() => void save()}
              >
                保存到文件夹…
              </Button>
            </div>
          </div>
        </section>
      ) : (
        <Text className="muted">选择来源并搜索玩家名，找到后可预览并保存皮肤。</Text>
      )}

      <div className="skin-site-browser">
        <div className="skin-site-copy">
          <Text weight="semibold">从皮肤网站挑选</Text>
          <Text size={200} className="muted">
            在网站里自行下载 PNG，再用「添加皮肤文件」导入。
          </Text>
        </div>
        <div className="skin-site-links" aria-label="皮肤网站">
          {sites.map((site) => (
            <Button
              key={site.key}
              size="small"
              appearance="subtle"
              icon={busy === `site:${site.key}` ? <Spinner size="tiny" /> : <Open16Regular />}
              disabled={locked}
              onClick={() => void openSite(site)}
            >
              在浏览器里打开 {site.label}
            </Button>
          ))}
        </div>
      </div>
    </section>
  );
}

/** Render the returned data URL as a complete, nearest-neighbour 2D figure. */
function PreviewFigure({ preview, scale = 7 }: { preview: SkinPreview; scale?: number }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [loaded, setLoaded] = useState<{ url: string; image: HTMLImageElement } | null>(null);

  useEffect(() => {
    let active = true;
    const image = new Image();
    image.onload = () => {
      if (active) {
        setLoaded({ url: preview.image, image });
      }
    };
    image.src = preview.image;
    return () => {
      active = false;
      image.onload = null;
    };
  }, [preview.image]);

  const image = loaded !== null && loaded.url === preview.image ? loaded.image : null;

  useEffect(() => {
    if (image !== null && canvas.current !== null) {
      drawFigure(canvas.current, image, preview.slim);
    }
  }, [image, preview.slim]);

  return (
    <canvas
      ref={canvas}
      className="skin-figure"
      width={figureSize.width * scale}
      height={figureSize.height * scale}
      style={{ width: figureSize.width * scale, height: figureSize.height * scale }}
      role="img"
      aria-label={`${preview.name} 的全身皮肤预览`}
    />
  );
}
