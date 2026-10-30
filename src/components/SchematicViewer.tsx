import { Button, Text } from "@fluentui/react-components";
import { Dismiss20Regular, Pin20Regular } from "@fluentui/react-icons";
import { useEffect, useMemo, useRef, useState } from "react";
import type { SchematicView } from "../ipc";
import { decodeRuns } from "../lib/schematic/decode";
import { VoxelGrid } from "../lib/schematic/grid";
import { describeHover, describeItem, indexEntities, nbtPreview } from "../lib/schematic/info";
import type { HoverInfo } from "../lib/schematic/info";
import type { FrameStats } from "../lib/schematic/perf";
import { SchematicRenderer } from "../lib/schematic/renderer";
import type { Keys } from "../lib/schematic/camera";

type SchematicViewerProps = {
  view: SchematicView;
  onClose: () => void;
};

const KEY_CODES: Record<string, keyof Keys> = {
  KeyW: "forward",
  KeyS: "back",
  KeyA: "left",
  KeyD: "right",
  Space: "up",
  ShiftLeft: "down",
  ShiftRight: "down",
  ControlLeft: "sprint",
  ControlRight: "sprint",
};

type Stats = { frame: FrameStats; meshed: number; total: number };

function buildGrid(view: SchematicView): VoxelGrid | Error {
  try {
    const [sx, sy, sz] = view.size;
    return new VoxelGrid(sx, sy, sz, decodeRuns(view.runs, view.runCount, sx * sy * sz), view.palette);
  } catch (error: unknown) {
    return error instanceof Error ? error : new Error("方块数据读不出来");
  }
}

/**
 * 投影的 3D 预览（全屏）。
 *
 * 操作：点击画面抓住鼠标开始转向；WASD 移动，空格上升，Shift 下降，Ctrl 加速，滚轮调速；
 * 准星（或没抓鼠标时的鼠标）指到的方块会显示它的名字、ID、方块状态、坐标，以及方块实体的
 * NBT 和容器里的物品；按 F 把当前方块固定到右边的检查面板（鼠标不被抓住时可以滚动、
 * 选中复制）；F3 看帧率和帧预算；Esc 先放开鼠标，再按一次关闭。
 *
 * 渲染的每一帧做什么、为什么能跟上显示器的刷新率，见 `lib/schematic/renderer.ts` 和
 * `docs/modules/schematics.md`。
 */
export function SchematicViewer({ view, onClose }: SchematicViewerProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rendererRef = useRef<SchematicRenderer | null>(null);
  const hoverRef = useRef<HoverInfo | null>(null);
  const lockedRef = useRef(false);

  const grid = useMemo(() => buildGrid(view), [view]);
  const entities = useMemo(() => indexEntities(view), [view]);

  const [hover, setHover] = useState<HoverInfo | null>(null);
  const [pinned, setPinned] = useState<HoverInfo | null>(null);
  const [locked, setLocked] = useState(false);
  const [showStats, setShowStats] = useState(false);
  const [stats, setStats] = useState<Stats | null>(null);
  const [failure, setFailure] = useState<string | null>(grid instanceof Error ? grid.message : null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (canvas === null || grid instanceof Error) {
      return;
    }
    let renderer: SchematicRenderer;
    try {
      renderer = new SchematicRenderer(canvas, grid, {
        onHover: (hit) => {
          const info = hit === null ? null : describeHover(view, entities, hit);
          hoverRef.current = info;
          setHover(info);
        },
        onStats: (frame, meshed, total) => setStats({ frame, meshed, total }),
        onLockChange: (value) => {
          lockedRef.current = value;
          setLocked(value);
        },
      });
    } catch {
      // 浏览器或显卡驱动不给 WebGL 时，`WebGLRenderer` 的构造会抛。effect 体里不能同步
      // 写 state，推到下一个微任务。
      void Promise.resolve().then(() => setFailure("这台电脑的浏览器环境不支持 WebGL，没法显示 3D 预览。"));
      return;
    }
    rendererRef.current = renderer;

    const observer = new ResizeObserver(() => renderer.resize());
    observer.observe(canvas);
    const onKey = (event: KeyboardEvent, down: boolean) => {
      const key = KEY_CODES[event.code];
      if (key !== undefined) {
        event.preventDefault();
        renderer.setKey(key, down);
        return;
      }
      if (!down) {
        return;
      }
      if (event.code === "F3") {
        event.preventDefault();
        setShowStats((value) => !value);
      } else if (event.code === "KeyF") {
        setPinned(hoverRef.current);
      } else if (event.code === "Escape" && !lockedRef.current) {
        // 鼠标被抓住时 Esc 由浏览器用来放开鼠标，不关闭预览。
        onClose();
      }
    };
    const onKeyDown = (event: KeyboardEvent) => onKey(event, true);
    const onKeyUp = (event: KeyboardEvent) => onKey(event, false);
    const onBlur = () => renderer.releaseKeys();
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("blur", onBlur);

    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      window.removeEventListener("blur", onBlur);
      observer.disconnect();
      renderer.dispose();
      rendererRef.current = null;
    };
  }, [grid, view, entities, onClose]);

  useEffect(() => {
    rendererRef.current?.setShowStats(showStats);
  }, [showStats]);

  const [sx, sy, sz] = view.size;

  return (
    <div className="viewer-overlay" role="dialog" aria-modal="true" aria-label={`${view.name || view.fileName} 的 3D 预览`}>
      <canvas
        ref={canvasRef}
        className="viewer-canvas"
        aria-label="3D 画面"
        onClick={() => rendererRef.current?.requestLock()}
        onMouseMove={(event) => {
          const box = event.currentTarget.getBoundingClientRect();
          rendererRef.current?.setMouse([event.clientX - box.left, event.clientY - box.top]);
        }}
        onMouseLeave={() => rendererRef.current?.setMouse(null)}
        onWheel={(event) => rendererRef.current?.scaleSpeed(event.deltaY < 0 ? 1.15 : 1 / 1.15)}
      />

      <header className="viewer-bar">
        <div className="viewer-title">
          <Text weight="semibold" size={400}>
            {view.name || view.fileName}
          </Text>
          <Text size={200} className="viewer-meta">
            {sx}×{sy}×{sz} · {view.solidBlocks.toLocaleString()} 个方块
            {view.dataVersion === null ? "" : ` · 数据版本 ${view.dataVersion}`}
          </Text>
        </div>
        <Button appearance="subtle" icon={<Dismiss20Regular />} aria-label="关闭预览" onClick={onClose} />
      </header>

      {failure !== null ? (
        <div className="viewer-failure" role="alert">
          <Text>{failure}</Text>
          <Button onClick={onClose}>关闭</Button>
        </div>
      ) : (
        <>
          <div className="viewer-crosshair" aria-hidden="true" data-locked={locked} />
          {hover !== null ? <HoverCard info={hover} /> : null}
          {pinned !== null ? <Inspector info={pinned} onClose={() => setPinned(null)} /> : null}
          {showStats && stats !== null ? <StatsPanel stats={stats} /> : null}
          <footer className="viewer-help">
            {locked
              ? "WASD 移动 · 空格 上升 · Shift 下降 · Ctrl 加速 · 滚轮调速 · F 固定方块信息 · F3 性能 · Esc 放开鼠标"
              : "点击画面开始转向 · WASD 移动 · 空格 上升 · Shift 下降 · Ctrl 加速 · F 固定 · F3 性能 · Esc 关闭"}
          </footer>
        </>
      )}
    </div>
  );
}

function PropertyList({ info }: { info: HoverInfo }) {
  if (info.properties.length === 0) {
    return null;
  }
  return (
    <ul className="jade-props">
      {info.properties.map((property) => (
        <li key={property.key}>
          <span className="jade-key">{property.key}</span>
          <span>{property.value}</span>
        </li>
      ))}
    </ul>
  );
}

/** Jade 风格的提示：指到哪个方块就显示它是什么、在哪、里面有什么。 */
function HoverCard({ info }: { info: HoverInfo }) {
  const preview = info.entity === null ? null : nbtPreview(info.entity.snbt);
  const items = info.entity?.items ?? [];
  return (
    <aside className="jade" aria-label="方块信息" aria-live="polite">
      <div className="jade-head">
        <span className="jade-swatch" style={{ background: info.swatch }} aria-hidden="true" />
        <div className="jade-names">
          <Text weight="semibold" size={400}>
            {info.title}
          </Text>
          <Text size={200} className="jade-id">
            {info.id}
          </Text>
        </div>
      </div>
      <Text size={200} className="jade-pos">
        x {info.position[0]} y {info.position[1]} z {info.position[2]}
        {info.face === null ? "" : ` 面：${info.face}`}
      </Text>
      <PropertyList info={info} />
      {info.entity !== null ? (
        <>
          <Text size={200} className="jade-entity">
            方块实体：{info.entity.id}
          </Text>
          {items.length > 0 ? (
            <ul className="jade-items" aria-label="容器内容">
              {items.slice(0, 8).map((item, index) => (
                <li key={`${item.slot}-${index}`}>
                  {item.slot === null ? "" : <span className="jade-key">#{item.slot}</span>}
                  <span>{describeItem(item)}</span>
                </li>
              ))}
              {items.length > 8 ? <li className="jade-more">…还有 {items.length - 8} 项</li> : null}
            </ul>
          ) : null}
          {preview !== null ? (
            <pre className="jade-nbt">
              {preview.text}
              {preview.hidden > 0 ? `\n…（还有 ${preview.hidden} 行）` : ""}
            </pre>
          ) : null}
          <Text size={100} className="jade-hint">
            按 F 固定，查看完整数据
          </Text>
        </>
      ) : (
        <Text size={100} className="jade-hint">
          按 F 固定
        </Text>
      )}
    </aside>
  );
}

/** 固定下来的方块的完整信息：鼠标不被抓住时可以滚动、选中、复制。 */
function Inspector({ info, onClose }: { info: HoverInfo; onClose: () => void }) {
  return (
    <aside className="inspector" aria-label="固定的方块">
      <div className="inspector-head">
        <Pin20Regular aria-hidden="true" />
        <Text weight="semibold">{info.title}</Text>
        <Button appearance="subtle" size="small" icon={<Dismiss20Regular />} aria-label="取消固定" onClick={onClose} />
      </div>
      <dl className="inspector-list">
        <div>
          <dt>ID</dt>
          <dd className="mono">{info.id}</dd>
        </div>
        <div>
          <dt>坐标</dt>
          <dd className="mono">
            {info.position[0]}, {info.position[1]}, {info.position[2]}
          </dd>
        </div>
        {info.properties.map((property) => (
          <div key={property.key}>
            <dt>{property.key}</dt>
            <dd className="mono">{property.value}</dd>
          </div>
        ))}
      </dl>
      {info.entity !== null ? (
        <>
          <Text weight="semibold" size={300}>
            {info.entity.id}
          </Text>
          {info.entity.items.length > 0 ? (
            <ul className="inspector-items" aria-label="容器内容">
              {info.entity.items.map((item, index) => (
                <li key={`${item.slot}-${index}`}>
                  <span>
                    {item.slot === null ? "" : `#${item.slot} `}
                    {describeItem(item)}
                  </span>
                  <span className="mono muted">{item.id}</span>
                  {item.data !== null ? <pre className="inspector-nbt">{item.data}</pre> : null}
                </li>
              ))}
            </ul>
          ) : null}
          <pre className="inspector-nbt" aria-label="完整 NBT" tabIndex={0}>
            {info.entity.snbt}
          </pre>
        </>
      ) : (
        <Text size={200} className="muted">
          这个方块没有方块实体数据。
        </Text>
      )}
    </aside>
  );
}

function StatsPanel({ stats }: { stats: Stats }) {
  const { frame } = stats;
  return (
    <aside className="viewer-stats mono" aria-label="性能">
      <div>
        显示器 {frame.refreshHz} Hz · 每帧预算 {frame.budgetMs.toFixed(1)} ms
      </div>
      <div>
        帧间隔 p95 {frame.intervalP95.toFixed(1)} ms · 本程序耗时 p95 {frame.workP95.toFixed(1)} ms
      </div>
      <div>
        掉帧 {frame.missedFrames} / {frame.totalFrames}
      </div>
      <div>
        画质 分辨率 {Math.round(frame.quality.renderScale * 100)}% · 距离 {Math.round(frame.quality.viewFraction * 100)}%
      </div>
      <div>
        网格 {stats.meshed} / {stats.total}
      </div>
    </aside>
  );
}
