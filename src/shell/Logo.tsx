import { useId } from "react";

type LogoProps = {
  size?: number;
  className?: string;
};

/**
 * Aether 的标志：一个等距的方块加一颗火花。与 `public/logo.svg` 和应用图标是同一个图形；
 * 渐变和遮罩的 ID 按实例生成，同一页里放多个 Logo 也不会互相串色。
 */
export function Logo({ size = 24, className }: LogoProps) {
  const uid = useId().replace(/[^a-zA-Z0-9]/g, "");
  const top = `${uid}top`;
  const left = `${uid}left`;
  const right = `${uid}right`;
  const seams = `${uid}seams`;

  return (
    <svg
      className={className}
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 512 512"
      width={size}
      height={size}
      role="img"
      aria-label="Aether Launcher"
    >
      <defs>
        <linearGradient id={top} x1="0.1" y1="0" x2="0.9" y2="1">
          <stop offset="0" stopColor="#8CF7E6" />
          <stop offset="1" stopColor="#58C7FF" />
        </linearGradient>
        <linearGradient id={left} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#2FC0DC" />
          <stop offset="1" stopColor="#3B78FF" />
        </linearGradient>
        <linearGradient id={right} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#4C66FF" />
          <stop offset="1" stopColor="#7B3FFF" />
        </linearGradient>
        <mask id={seams} maskUnits="userSpaceOnUse" x="0" y="0" width="512" height="512">
          <rect width="512" height="512" fill="#fff" />
          <path
            d="M256 260 L91.5 165 M256 260 L420.5 165 M256 260 L256 450"
            stroke="#000"
            strokeWidth="9"
            fill="none"
          />
        </mask>
      </defs>
      <g mask={`url(#${seams})`} strokeLinejoin="round" strokeWidth="18">
        <polygon
          points="256,70 420.5,165 256,260 91.5,165"
          fill={`url(#${top})`}
          stroke={`url(#${top})`}
        />
        <polygon
          points="91.5,165 256,260 256,450 91.5,355"
          fill={`url(#${left})`}
          stroke={`url(#${left})`}
        />
        <polygon
          points="420.5,165 420.5,355 256,450 256,260"
          fill={`url(#${right})`}
          stroke={`url(#${right})`}
        />
      </g>
      <path
        d="M392 52C394 80 405 91 434 93C405 95 394 106 392 134C390 106 379 95 350 93C379 91 390 80 392 52Z"
        fill={`url(#${top})`}
      />
    </svg>
  );
}
