/**
 * 飞行相机：WASD 水平移动，空格上升，Shift 下降，Ctrl 加速，鼠标转向。
 *
 * 约定：偏航 0 朝 -z（北），向右转偏航增大；俯仰向上为正。坐标系和 Minecraft 一致
 * （x 东、y 上、z 南）。全是纯函数，不碰 DOM 也不分配对象以外的东西，方便单测。
 */

export type FlyState = {
  x: number;
  y: number;
  z: number;
  yaw: number;
  pitch: number;
};

export type Keys = {
  forward: boolean;
  back: boolean;
  left: boolean;
  right: boolean;
  up: boolean;
  down: boolean;
  sprint: boolean;
};

export const noKeys: Keys = {
  forward: false,
  back: false,
  left: false,
  right: false,
  up: false,
  down: false,
  sprint: false,
};

const MAX_PITCH = (89 * Math.PI) / 180;
const SPRINT_FACTOR = 3;

/** 视线方向（单位向量）。 */
export function lookDirection(yaw: number, pitch: number): [number, number, number] {
  const cosPitch = Math.cos(pitch);
  return [Math.sin(yaw) * cosPitch, Math.sin(pitch), -Math.cos(yaw) * cosPitch];
}

/** 鼠标位移 → 新的朝向。俯仰夹在 ±89°，偏航保持在 (-π, π]。 */
export function applyLook(state: FlyState, dx: number, dy: number, sensitivity = 0.0022): FlyState {
  let yaw = state.yaw + dx * sensitivity;
  yaw = ((((yaw + Math.PI) % (2 * Math.PI)) + 2 * Math.PI) % (2 * Math.PI)) - Math.PI;
  const pitch = Math.max(-MAX_PITCH, Math.min(MAX_PITCH, state.pitch - dy * sensitivity));
  return { ...state, yaw, pitch };
}

/**
 * 按住的键 → 位移。水平方向沿视线的水平分量（所以抬头走路不会飞起来），斜着走不会更快；
 * 速度单位是方块/秒，`dt` 是秒。
 */
export function stepFly(state: FlyState, keys: Keys, dt: number, speed: number): FlyState {
  const forward = Number(keys.forward) - Number(keys.back);
  const strafe = Number(keys.right) - Number(keys.left);
  const rise = Number(keys.up) - Number(keys.down);
  if (forward === 0 && strafe === 0 && rise === 0) {
    return state;
  }
  const length = Math.hypot(forward, strafe, rise);
  const pace = (speed * (keys.sprint ? SPRINT_FACTOR : 1) * Math.max(0, dt)) / length;
  const sin = Math.sin(state.yaw);
  const cos = Math.cos(state.yaw);
  return {
    ...state,
    x: state.x + (sin * forward + cos * strafe) * pace,
    y: state.y + rise * pace,
    z: state.z + (-cos * forward + sin * strafe) * pace,
  };
}

/** 默认速度：大的建筑走得快一些，大约十秒走完最长的一边。 */
export function defaultSpeed(size: readonly [number, number, number]): number {
  return Math.max(6, Math.max(...size) / 10);
}

/** 起始位置：从一个角上方斜着往中心看，整个建筑刚好在视野里。 */
export function startPose(size: readonly [number, number, number]): FlyState {
  const [sx, sy, sz] = size;
  const reach = Math.max(sx, sy, sz);
  const centre: [number, number, number] = [sx / 2, sy / 2, sz / 2];
  const position: [number, number, number] = [
    centre[0] + reach * 0.9,
    centre[1] + reach * 0.7,
    centre[2] + reach * 1.2,
  ];
  const dx = centre[0] - position[0];
  const dy = centre[1] - position[1];
  const dz = centre[2] - position[2];
  return {
    x: position[0],
    y: position[1],
    z: position[2],
    yaw: Math.atan2(dx, -dz),
    pitch: Math.atan2(dy, Math.hypot(dx, dz)),
  };
}
