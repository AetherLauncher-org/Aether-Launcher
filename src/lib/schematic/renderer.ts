import {
  BoxGeometry,
  BufferAttribute,
  BufferGeometry,
  Color,
  EdgesGeometry,
  LineBasicMaterial,
  LineSegments,
  Mesh,
  MeshBasicMaterial,
  PerspectiveCamera,
  Scene,
  Sphere,
  Vector3,
  WebGLRenderer,
} from "three";
import { applyLook, defaultSpeed, lookDirection, noKeys, startPose, stepFly } from "./camera";
import type { FlyState, Keys } from "./camera";
import type { VoxelGrid } from "./grid";
import { CHUNK, ChunkJob, chunkCounts } from "./mesher";
import type { MeshData } from "./mesher";
import { FrameBudget } from "./perf";
import type { FrameStats } from "./perf";
import { pick } from "./picker";
import type { Hit } from "./picker";

export type RendererEvents = {
  /** 指到的方块变了（`null` 表示什么都没指到） */
  onHover: (hit: Hit | null) => void;
  /** 大约每 250 毫秒一次，只在 `showStats` 打开时 */
  onStats: (stats: FrameStats, meshedChunks: number, totalChunks: number) => void;
  /** 鼠标有没有被相机抓住 */
  onLockChange: (locked: boolean) => void;
};

type ChunkMeshes = { opaque: Mesh | null; translucent: Mesh | null; centre: Vector3 };

const SKY = new Color(0x9ec7ff);
/** 一块网格的外接球半径：边长 16 的立方体的半个体对角线。 */
const CHUNK_RADIUS = (Math.sqrt(3) * CHUNK) / 2;
/** 相机挪动超过这么多方块才重新按距离给待建的块排序。 */
const RESORT_DISTANCE = 6;
const STATS_INTERVAL_MS = 250;

/**
 * 把一个 `VoxelGrid` 画出来，并且在每一帧里只做有上限的事。
 *
 * 每帧：移动相机；在帧预算给的时间片里建网格（一块网格分六步，步与步之间可以停）；
 * 最多往 GPU 上传一块；只有画面变了才重绘；只有视线或鼠标动了才指一次方块。预算、
 * 画质档位和掉帧计数在 `FrameBudget` 里。
 */
export class SchematicRenderer {
  private readonly renderer: WebGLRenderer;
  private readonly scene = new Scene();
  private readonly camera = new PerspectiveCamera(70, 1, 0.1, 1000);
  private readonly budget = new FrameBudget();
  private readonly chunks = new Map<number, ChunkMeshes>();
  private readonly opaqueMaterial = new MeshBasicMaterial({ vertexColors: true });
  private readonly glassMaterial = new MeshBasicMaterial({
    vertexColors: true,
    transparent: true,
    opacity: 0.55,
    depthWrite: false,
  });
  private readonly outline: LineSegments;
  private readonly counts: [number, number, number];
  private readonly far: number;

  private pose: FlyState;
  private speed: number;
  private keys: Keys = { ...noKeys };
  private locked = false;
  private mouse: [number, number] | null = null;

  private queue: number[] = [];
  private job: ChunkJob | null = null;
  private sortedAt = new Vector3(Infinity, 0, 0);
  private meshed = 0;

  private dirty = true;
  private moved = true;
  private hoverDirty = true;
  private hit: Hit | null = null;
  private showStats = false;
  private statsAt = 0;
  private lastNow = 0;
  private appliedScale = -1;
  private raf = 0;
  private disposed = false;

  constructor(
    private readonly canvas: HTMLCanvasElement,
    private readonly grid: VoxelGrid,
    private readonly events: RendererEvents,
  ) {
    this.renderer = new WebGLRenderer({
      canvas,
      antialias: false,
      alpha: false,
      stencil: false,
      powerPreference: "high-performance",
    });
    this.renderer.setClearColor(SKY);
    this.renderer.sortObjects = false;
    this.renderer.setPixelRatio(1);

    const size: [number, number, number] = [grid.sx, grid.sy, grid.sz];
    this.counts = chunkCounts(grid);
    this.pose = startPose(size);
    this.speed = defaultSpeed(size);
    this.far = Math.hypot(...size) * 2 + 100;
    this.camera.far = this.far;
    this.camera.rotation.order = "YXZ";

    const outlineGeometry = new EdgesGeometry(new BoxGeometry(1.004, 1.004, 1.004));
    this.outline = new LineSegments(outlineGeometry, new LineBasicMaterial({ color: 0x000000 }));
    this.outline.visible = false;
    this.scene.add(this.outline);

    this.queueChunks();
    this.bind();
    this.resize();
    this.raf = requestAnimationFrame(this.tick);
  }

  // ---- 外部控制 ----------------------------------------------------------------------

  setShowStats(show: boolean): void {
    this.showStats = show;
  }

  setKey(key: keyof Keys, down: boolean): void {
    if (this.keys[key] !== down) {
      this.keys = { ...this.keys, [key]: down };
    }
  }

  releaseKeys(): void {
    this.keys = { ...noKeys };
  }

  /** 调速：滚轮向上变快。 */
  scaleSpeed(factor: number): void {
    this.speed = Math.min(2000, Math.max(1, this.speed * factor));
  }

  requestLock(): void {
    void this.canvas.requestPointerLock?.();
  }

  releaseLock(): void {
    if (document.pointerLockElement === this.canvas) {
      document.exitPointerLock();
    }
  }

  /** 屏幕上的鼠标位置（相对画布）；不在画布上用 `null`。 */
  setMouse(position: [number, number] | null): void {
    this.mouse = position;
    this.hoverDirty = true;
  }

  resize(): void {
    this.appliedScale = -1;
    this.dirty = true;
    this.hoverDirty = true;
  }

  dispose(): void {
    this.disposed = true;
    cancelAnimationFrame(this.raf);
    this.unbind();
    for (const parts of this.chunks.values()) {
      for (const mesh of [parts.opaque, parts.translucent]) {
        mesh?.geometry.dispose();
      }
    }
    this.chunks.clear();
    this.outline.geometry.dispose();
    (this.outline.material as LineBasicMaterial).dispose();
    this.opaqueMaterial.dispose();
    this.glassMaterial.dispose();
    this.renderer.dispose();
  }

  // ---- 事件 -------------------------------------------------------------------------

  /** 转向：鼠标位移（像素）。抓取鼠标时来自 `movementX/Y`，拖动转向时来自指针位移。 */
  look(dx: number, dy: number): void {
    this.pose = applyLook(this.pose, dx, dy);
    this.moved = true;
    this.dirty = true;
    this.hoverDirty = true;
  }

  private readonly onMouseMove = (event: MouseEvent): void => {
    if (this.locked) {
      this.look(event.movementX, event.movementY);
    }
  };

  private readonly onLockChange = (): void => {
    this.locked = document.pointerLockElement === this.canvas;
    this.hoverDirty = true;
    this.events.onLockChange(this.locked);
  };

  private bind(): void {
    document.addEventListener("mousemove", this.onMouseMove);
    document.addEventListener("pointerlockchange", this.onLockChange);
  }

  private unbind(): void {
    document.removeEventListener("mousemove", this.onMouseMove);
    document.removeEventListener("pointerlockchange", this.onLockChange);
    this.releaseLock();
  }

  // ---- 每一帧 -----------------------------------------------------------------------

  private readonly tick = (now: number): void => {
    if (this.disposed) {
      return;
    }
    this.raf = requestAnimationFrame(this.tick);
    const started = performance.now();
    const interval = this.lastNow === 0 ? 0 : now - this.lastNow;
    this.lastNow = now;

    this.move(Math.min(0.1, interval / 1000));
    this.applyQuality();
    this.buildSlice();
    this.updateVisibility();
    if (this.dirty) {
      this.renderer.render(this.scene, this.camera);
      this.dirty = false;
    }
    this.updateHover();

    const work = performance.now() - started;
    if (!document.hidden) {
      this.budget.frame(interval, work);
    }
    if (this.showStats && now - this.statsAt >= STATS_INTERVAL_MS) {
      this.statsAt = now;
      this.events.onStats(this.budget.stats(), this.meshed, this.counts[0] * this.counts[1] * this.counts[2]);
    }
  };

  private move(dt: number): void {
    const next = stepFly(this.pose, this.keys, dt, this.speed);
    if (next !== this.pose) {
      this.pose = next;
      this.moved = true;
      this.dirty = true;
      this.hoverDirty = true;
    }
    if (this.moved) {
      const { x, y, z, yaw, pitch } = this.pose;
      this.camera.position.set(x, y, z);
      this.camera.rotation.set(pitch, -yaw, 0);
      this.camera.updateMatrixWorld();
    }
  }

  /** 画质档位变了（或窗口大小变了）才改渲染分辨率。 */
  private applyQuality(): void {
    const { renderScale } = this.budget.quality;
    if (renderScale === this.appliedScale) {
      return;
    }
    this.appliedScale = renderScale;
    const width = Math.max(1, this.canvas.clientWidth);
    const height = Math.max(1, this.canvas.clientHeight);
    this.renderer.setSize(Math.max(1, Math.floor(width * renderScale)), Math.max(1, Math.floor(height * renderScale)), false);
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
    this.dirty = true;
  }

  // ---- 建网格 -----------------------------------------------------------------------

  private queueChunks(): void {
    const [nx, ny, nz] = this.counts;
    this.queue = [];
    for (let cz = 0; cz < nz; cz += 1) {
      for (let cy = 0; cy < ny; cy += 1) {
        for (let cx = 0; cx < nx; cx += 1) {
          this.queue.push((cz * ny + cy) * nx + cx);
        }
      }
    }
    this.sortedAt.set(Infinity, 0, 0);
  }

  private chunkCoords(key: number): [number, number, number] {
    const [nx, ny] = this.counts;
    return [key % nx, Math.floor(key / nx) % ny, Math.floor(key / (nx * ny))];
  }

  private chunkCentre(key: number): Vector3 {
    const [cx, cy, cz] = this.chunkCoords(key);
    return new Vector3((cx + 0.5) * CHUNK, (cy + 0.5) * CHUNK, (cz + 0.5) * CHUNK);
  }

  /** 离相机远的排在前面，`pop()` 取最近的。 */
  private sortQueue(): void {
    const camera = this.camera.position;
    if (camera.distanceTo(this.sortedAt) < RESORT_DISTANCE) {
      return;
    }
    this.sortedAt.copy(camera);
    const distance = new Map<number, number>();
    for (const key of this.queue) {
      distance.set(key, this.chunkCentre(key).distanceToSquared(camera));
    }
    this.queue.sort((a, b) => (distance.get(b) ?? 0) - (distance.get(a) ?? 0));
  }

  /** 在这一帧的时间片里推进建网格：一步一步做，时间到了就停，下一帧接着做。 */
  private buildSlice(): void {
    const allowance = this.budget.meshBudgetMs;
    if (allowance <= 0 || (this.job === null && this.queue.length === 0)) {
      return;
    }
    this.sortQueue();
    const deadline = performance.now() + allowance;
    let uploaded = false;
    while (performance.now() < deadline && !uploaded) {
      if (this.job === null) {
        const key = this.queue.pop();
        if (key === undefined) {
          return;
        }
        const [cx, cy, cz] = this.chunkCoords(key);
        this.job = new ChunkJob(this.grid, cx, cy, cz);
      }
      this.job.step();
      if (this.job.done) {
        const mesh = this.job.result();
        const key = (mesh.cz * this.counts[1] + mesh.cy) * this.counts[0] + mesh.cx;
        this.job = null;
        this.meshed += 1;
        // 空的块没有东西要上传，接着做下一块；有东西的这一帧只传这一块。
        if (mesh.opaque !== null || mesh.translucent !== null) {
          this.upload(key, mesh.opaque, mesh.translucent);
          uploaded = true;
        }
      }
    }
  }

  private upload(key: number, opaque: MeshData | null, translucent: MeshData | null): void {
    const centre = this.chunkCentre(key);
    const parts: ChunkMeshes = {
      opaque: opaque === null ? null : this.makeMesh(opaque, this.opaqueMaterial, centre, 0),
      translucent: translucent === null ? null : this.makeMesh(translucent, this.glassMaterial, centre, 1),
      centre,
    };
    this.chunks.set(key, parts);
    this.dirty = true;
  }

  private makeMesh(data: MeshData, material: MeshBasicMaterial, centre: Vector3, order: number): Mesh {
    const geometry = new BufferGeometry();
    geometry.setAttribute("position", new BufferAttribute(data.positions, 3));
    geometry.setAttribute("color", new BufferAttribute(data.colors, 3, true));
    geometry.setIndex(new BufferAttribute(data.indices, 1));
    geometry.boundingSphere = new Sphere(centre.clone(), CHUNK_RADIUS);
    const mesh = new Mesh(geometry, material);
    mesh.renderOrder = order;
    mesh.matrixAutoUpdate = false;
    this.scene.add(mesh);
    return mesh;
  }

  /** 超出可见距离的块不画。相机动了才重新算。 */
  private updateVisibility(): void {
    if (!this.moved) {
      return;
    }
    this.moved = false;
    const limit = this.far * this.budget.quality.viewFraction + CHUNK_RADIUS;
    const limitSquared = limit * limit;
    const camera = this.camera.position;
    for (const parts of this.chunks.values()) {
      const visible = parts.centre.distanceToSquared(camera) <= limitSquared;
      for (const mesh of [parts.opaque, parts.translucent]) {
        if (mesh !== null && mesh.visible !== visible) {
          mesh.visible = visible;
          this.dirty = true;
        }
      }
    }
  }

  // ---- 指方块 -----------------------------------------------------------------------

  private updateHover(): void {
    if (!this.hoverDirty) {
      return;
    }
    this.hoverDirty = false;
    const origin: [number, number, number] = [this.pose.x, this.pose.y, this.pose.z];
    let direction: [number, number, number];
    if (this.locked || this.mouse === null) {
      direction = lookDirection(this.pose.yaw, this.pose.pitch);
    } else {
      const width = Math.max(1, this.canvas.clientWidth);
      const height = Math.max(1, this.canvas.clientHeight);
      const point = new Vector3((this.mouse[0] / width) * 2 - 1, -(this.mouse[1] / height) * 2 + 1, 0.5).unproject(this.camera);
      point.sub(this.camera.position).normalize();
      direction = [point.x, point.y, point.z];
    }
    const hit = pick(this.grid, origin, direction, this.far);
    if ((hit?.index ?? -1) === (this.hit?.index ?? -1)) {
      return;
    }
    this.hit = hit;
    if (hit === null) {
      this.outline.visible = false;
    } else {
      this.outline.position.set(hit.x + 0.5, hit.y + 0.5, hit.z + 0.5);
      this.outline.visible = true;
    }
    this.dirty = true;
    this.events.onHover(hit);
  }
}
