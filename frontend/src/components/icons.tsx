// 页内图标体系：与 Logo（S + 播放三角，蓝青紫渐变）同风格的线性图标
// 规范：24 viewBox / stroke=currentColor / 1.8 描边 / 圆角端点（产品方案 2.5 线性图标体系）
import type { ReactNode } from 'react';

interface IconProps {
  size?: number;
  className?: string;
  strokeWidth?: number;
}

function Svg({ children, size = 18, className = '', strokeWidth = 1.8 }: IconProps & { children: ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

/* ---------- 品牌 ---------- */

/** Logo 图形复刻：渐变弧带 S + 白色播放三角 */
export function LogoMark({ size = 36 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <defs>
        <linearGradient id="z-brand" x1="3" y1="4" x2="21" y2="20" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor="#ffffff" />
          <stop offset="1" stopColor="#a1a1aa" />
        </linearGradient>
      </defs>
      <rect x="1" y="1" width="22" height="22" rx="6" fill="#18181b" />
      {/* Z 字：粗笔画三段，斜切端点 */}
      <path d="M 6.3 5.4 L 17.6 5.4 L 16.2 8.0 L 6.3 8.0 Z" fill="url(#z-brand)" />
      <path d="M 17.6 5.4 L 7.9 16.3 L 6.3 16.3 L 6.3 15.7 L 16.2 5.4 Z" fill="url(#z-brand)" />
      <path d="M 6.3 15.7 L 17.7 15.7 L 17.7 18.4 L 7.6 18.4 Z" fill="url(#z-brand)" />
    </svg>
  );
}

/* ---------- 导航 ---------- */

export function HomeIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M3 10.8 12 3.5l9 7.3" />
      <path d="M5.2 9.8V19a1.5 1.5 0 0 0 1.5 1.5h10.6a1.5 1.5 0 0 0 1.5-1.5V9.8" />
      <path d="M9.8 20.5v-6h4.4v6" />
    </Svg>
  );
}

export function CompassIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <circle cx="12" cy="12" r="9" />
      <path d="m15.9 8.1-1.9 5.4-5.4 1.9 1.9-5.4z" />
    </Svg>
  );
}

export function StarIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="m12 3.6 2.5 5.2 5.7.8-4.1 4 1 5.7-5.1-2.7-5.1 2.7 1-5.7-4.1-4 5.7-.8z" />
    </Svg>
  );
}

export function StarFilledIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path
        d="m12 3.6 2.5 5.2 5.7.8-4.1 4 1 5.7-5.1-2.7-5.1 2.7 1-5.7-4.1-4 5.7-.8z"
        fill="currentColor"
        stroke="none"
      />
    </Svg>
  );
}

export function HistoryIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7v5l3.2 2.4" />
    </Svg>
  );
}

export function PlayIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M8.5 5.5v13c0 .8.9 1.3 1.5.9l10-6.5c.6-.4.6-1.4 0-1.8l-10-6.5c-.6-.4-1.5.1-1.5.9z" fill="currentColor" stroke="none" />
    </Svg>
  );
}

/* ---------- 操作 ---------- */

export function SearchIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <circle cx="11" cy="11" r="7" />
      <path d="m16.2 16.2 4.3 4.3" />
    </Svg>
  );
}

export function RefreshIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M20.4 12a8.4 8.4 0 1 1-2.5-6" />
      <path d="M20.5 3.5v5h-5" />
    </Svg>
  );
}

export function VolumeIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M11 5.5 6.5 9H3.5v6h3L11 18.5z" />
      <path d="M14.8 9a4.3 4.3 0 0 1 0 6" />
      <path d="M17.5 6.7a7.5 7.5 0 0 1 0 10.6" />
    </Svg>
  );
}

export function MuteIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M11 5.5 6.5 9H3.5v6h3L11 18.5z" />
      <path d="m15.5 9.5 5 5" />
      <path d="m20.5 9.5-5 5" />
    </Svg>
  );
}

export function MaximizeIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M8.5 3.5H5.5a2 2 0 0 0-2 2v3" />
      <path d="M15.5 3.5h3a2 2 0 0 1 2 2v3" />
      <path d="M8.5 20.5H5.5a2 2 0 0 1-2-2v-3" />
      <path d="M15.5 20.5h3a2 2 0 0 0 2-2v-3" />
    </Svg>
  );
}

export function ExternalLinkIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M13.5 5H6.5a2 2 0 0 0-2 2v10.5a2 2 0 0 0 2 2H17a2 2 0 0 0 2-2v-7" />
      <path d="M14.5 3.5h6v6" />
      <path d="M10 14 20.5 3.5" />
    </Svg>
  );
}

export function AlertIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M10.3 4 2.6 17.5A1.9 1.9 0 0 0 4.3 20.3h15.4a1.9 1.9 0 0 0 1.7-2.8L13.7 4a1.9 1.9 0 0 0-3.4 0z" />
      <path d="M12 9.3v4" />
      <path d="M12 16.3v.1" />
    </Svg>
  );
}

export function CloseIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="m6 6 12 12" />
      <path d="m18 6-12 12" />
    </Svg>
  );
}

export function SignalIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <circle cx="12" cy="12" r="2.2" fill="currentColor" stroke="none" />
      <path d="M8.5 15.5a5 5 0 0 1 0-7" />
      <path d="M15.5 8.5a5 5 0 0 1 0 7" />
      <path d="M5.6 18.4a9 9 0 0 1 0-12.8" />
      <path d="M18.4 5.6a9 9 0 0 1 0 12.8" />
    </Svg>
  );
}

export function SettingsIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1 1.55V21a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1-1.55 1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.7 1.7 0 0 0 .34-1.87 1.7 1.7 0 0 0-1.55-1H3a2 2 0 1 1 0-4h.09a1.7 1.7 0 0 0 1.55-1 1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.87.34h.09a1.7 1.7 0 0 0 1-1.55V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1 1.55 1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.87v.09a1.7 1.7 0 0 0 1.55 1H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.55 1z" />
    </Svg>
  );
}

/* ---------- 主题 ---------- */

export function SunIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2.5v2" />
      <path d="M12 19.5v2" />
      <path d="M3.5 12h2" />
      <path d="M18.5 12h2" />
      <path d="m5.4 5.4 1.4 1.4" />
      <path d="m17.2 17.2 1.4 1.4" />
      <path d="m18.6 5.4-1.4 1.4" />
      <path d="m6.8 17.2-1.4 1.4" />
    </Svg>
  );
}

export function MoonIcon(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M20.5 14.2A8.5 8.5 0 0 1 9.8 3.5 8.5 8.5 0 1 0 20.5 14.2z" />
    </Svg>
  );
}
