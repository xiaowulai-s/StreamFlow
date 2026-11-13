import { useCallback, useEffect, useRef, useState } from 'react';
import { getSetting, setSetting } from '../lib/tauri';
import { onDanmakuMessage } from '../lib/tauri';
import type { DanmakuMessage } from '../lib/tauri';

/** 飘屏配置（持久化于 settings: danmaku_overlay_config） */
export interface OverlayConfig {
  enabled: boolean;
  /** 弹幕层高度占画面比例 0.1~1 */
  range: number;
  /** 同屏最大条数 */
  maxCount: number;
  /** 横穿时长（秒） */
  speedSec: number;
  /** 文字透明度 0.3~1 */
  opacity: number;
  /** 字号 px */
  fontSize: number;
}

export const DEFAULT_CONFIG: OverlayConfig = {
  enabled: true,
  range: 0.25,
  maxCount: 15,
  speedSec: 8,
  opacity: 0.85,
  fontSize: 18,
};

export const OVERLAY_PRESETS: Array<{ label: string; range: number }> = [
  { label: '1/4 屏', range: 0.25 },
  { label: '半屏', range: 0.5 },
  { label: '3/4 屏', range: 0.75 },
  { label: '全屏', range: 1 },
];

export function loadOverlayConfig(): Promise<OverlayConfig> {
  return getSetting('danmaku_overlay_config')
    .then((v) => {
      if (!v) return DEFAULT_CONFIG;
      const parsed = JSON.parse(v) as Partial<OverlayConfig>;
      return { ...DEFAULT_CONFIG, ...parsed };
    })
    .catch(() => DEFAULT_CONFIG);
}

export function saveOverlayConfig(cfg: OverlayConfig): void {
  void setSetting('danmaku_overlay_config', JSON.stringify(cfg)).catch(() => undefined);
}

interface FloatingItem {
  key: number;
  username: string;
  text: string;
  row: number;
  color?: string | null;
}

/** 行高（相对弹幕层高度）：字号 1.6 倍，最小 26px */
function rowHeightPx(fontSize: number): number {
  return Math.max(26, Math.round(fontSize * 1.6));
}

/**
 * 视频画面横向滚动弹幕（飘屏）层。
 * 绝对定位覆盖播放器，pointer-events 关闭不挡交互；与右侧列表共用弹幕事件源，互不影响。
 */
export function DanmakuOverlay({
  platform,
  roomId,
  config,
}: {
  platform: string;
  roomId: string;
  config: OverlayConfig;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [items, setItems] = useState<FloatingItem[]>([]);
  const keyRef = useRef(0);
  const rowCountRef = useRef(1);
  const rowNextFreeRef = useRef<number[]>([0]);
  const countRef = useRef(0);
  const configRef = useRef(config);
  configRef.current = config;

  // 行数随容器高度/字号/范围变化重算
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const update = () => {
      const h = el.clientHeight * configRef.current.range;
      const rows = Math.max(1, Math.floor(h / rowHeightPx(configRef.current.fontSize)));
      rowCountRef.current = rows;
      if (rowNextFreeRef.current.length < rows) {
        rowNextFreeRef.current = [
          ...rowNextFreeRef.current,
          ...Array.from({ length: rows - rowNextFreeRef.current.length }, () => 0),
        ];
      } else {
        rowNextFreeRef.current = rowNextFreeRef.current.slice(0, rows);
      }
    };
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, [config.range, config.fontSize]);

  // 订阅弹幕事件（与右侧列表并行）
  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    (async () => {
      unlisteners.push(
        await onDanmakuMessage((msg: DanmakuMessage) => {
          if (disposed) return;
          if (msg.platform !== platform || msg.roomId !== roomId) return;
          const cfg = configRef.current;
          if (!cfg.enabled) return;
          // 密度上限：超限丢弃新消息（防遮挡）
          if (countRef.current >= cfg.maxCount) return;
          // 行分配：只进"前一条尾部已离开右边缘"的空闲行；无空闲行则丢弃（绝不重叠）。
          // 空闲判定：该行上一条进入后已飞行 speed×0.55（尾部恰好离开右缘，后条紧挨前条）。
          const rows = rowCountRef.current;
          const now = Date.now();
          let targetRow = -1;
          for (let i = 0; i < rows; i++) {
            if ((rowNextFreeRef.current[i] ?? 0) <= now) {
              targetRow = i;
              break;
            }
          }
          if (targetRow < 0) return;
          rowNextFreeRef.current[targetRow] = now + cfg.speedSec * 1000 * 0.55;
          const key = ++keyRef.current;
          countRef.current += 1;
          setItems((prev) => [
            ...prev.slice(-59),
            { key, username: msg.username, text: msg.text, row: targetRow, color: msg.color },
          ]);
        }),
      );
    })();
    return () => {
      disposed = true;
      unlisteners.forEach((fn) => fn());
      countRef.current = 0;
      setItems([]);
    };
  }, [platform, roomId]);

  const removeItem = useCallback((key: number) => {
    countRef.current = Math.max(0, countRef.current - 1);
    setItems((prev) => prev.filter((it) => it.key !== key));
  }, []);

  if (!config.enabled) return null;
  const rowH = rowHeightPx(config.fontSize);

  return (
    <div
      ref={containerRef}
      className="pointer-events-none absolute inset-x-0 top-0 overflow-hidden"
      style={{ height: `${Math.round(config.range * 100)}%` }}
      aria-hidden="true"
    >
      {items.map((it) => (
        <div
          key={it.key}
          className="absolute whitespace-nowrap rounded bg-black/30 px-2 will-change-transform"
          style={{
            top: it.row * rowH,
            left: '100%',
            fontSize: config.fontSize,
            lineHeight: `${rowH}px`,
            color: it.color ?? '#fff',
            opacity: config.opacity,
            textShadow: '0 1px 2px rgba(0,0,0,.8)',
            animation: `dm-slide ${config.speedSec}s linear forwards`,
            ['--dm-dist' as string]: '110vw',
          }}
          onAnimationEnd={() => removeItem(it.key)}
        >
          {it.text}
        </div>
      ))}
    </div>
  );
}

/** 飘屏控制面板（弹幕区顶部工具条弹出层用） */
export function OverlaySettings({
  config,
  onChange,
}: {
  config: OverlayConfig;
  onChange: (cfg: OverlayConfig) => void;
}) {
  const set = <K extends keyof OverlayConfig>(k: K, v: OverlayConfig[K]) => {
    const next = { ...config, [k]: v };
    onChange(next);
    saveOverlayConfig(next);
  };
  return (
    <div className="w-64 space-y-3 rounded-card bg-panel p-3 text-xs text-ink ring-1 ring-black/10">
      <label className="flex items-center justify-between">
        <span>飘屏开关</span>
        <input
          type="checkbox"
          checked={config.enabled}
          onChange={(e) => set('enabled', e.target.checked)}
          className="h-4 w-4"
        />
      </label>
      <div>
        <div className="mb-1 flex justify-between">
          <span>显示范围</span>
          <span>{Math.round(config.range * 100)}%</span>
        </div>
        <div className="mb-1 flex gap-1">
          {OVERLAY_PRESETS.map((p) => (
            <button
              key={p.label}
              type="button"
              onClick={() => set('range', p.range)}
              className={`flex-1 rounded px-1 py-0.5 ring-1 ${
                Math.abs(config.range - p.range) < 0.01
                  ? 'bg-accent text-white ring-accent'
                  : 'ring-black/15 hover:bg-black/5'
              }`}
            >
              {p.label}
            </button>
          ))}
        </div>
        <input
          type="range"
          min={10}
          max={100}
          value={Math.round(config.range * 100)}
          onChange={(e) => set('range', Number(e.target.value) / 100)}
          className="w-full"
        />
      </div>
      <label className="flex items-center justify-between gap-2">
        <span>密度上限</span>
        <span className="flex items-center gap-1">
          <input
            type="range"
            min={5}
            max={30}
            value={config.maxCount}
            onChange={(e) => set('maxCount', Number(e.target.value))}
            className="w-24"
          />
          <b>{config.maxCount}</b>
        </span>
      </label>
      <label className="flex items-center justify-between gap-2">
        <span>速度</span>
        <span className="flex gap-1">
          {[
            { l: '慢', s: 12 },
            { l: '中', s: 8 },
            { l: '快', s: 5 },
          ].map((o) => (
            <button
              key={o.l}
              type="button"
              onClick={() => set('speedSec', o.s)}
              className={`rounded px-2 py-0.5 ring-1 ${
                config.speedSec === o.s
                  ? 'bg-accent text-white ring-accent'
                  : 'ring-black/15 hover:bg-black/5'
              }`}
            >
              {o.l}
            </button>
          ))}
        </span>
      </label>
      <label className="flex items-center justify-between gap-2">
        <span>透明度</span>
        <span className="flex items-center gap-1">
          <input
            type="range"
            min={30}
            max={100}
            value={Math.round(config.opacity * 100)}
            onChange={(e) => set('opacity', Number(e.target.value) / 100)}
            className="w-24"
          />
          <b>{Math.round(config.opacity * 100)}%</b>
        </span>
      </label>
      <label className="flex items-center justify-between gap-2">
        <span>字号</span>
        <span className="flex gap-1">
          {[
            { l: '小', s: 14 },
            { l: '中', s: 18 },
            { l: '大', s: 24 },
          ].map((o) => (
            <button
              key={o.l}
              type="button"
              onClick={() => set('fontSize', o.s)}
              className={`rounded px-2 py-0.5 ring-1 ${
                config.fontSize === o.s
                  ? 'bg-accent text-white ring-accent'
                  : 'ring-black/15 hover:bg-black/5'
              }`}
            >
              {o.l}
            </button>
          ))}
        </span>
      </label>
    </div>
  );
}
