// 直播卡片：16:9 封面占位 + 平台徽标 + 状态（产品方案 5.2）
// 当前各平台接口未提供封面图，使用渐变占位 + 主播名首字；接口提供封面后替换 img
import type { Room } from '../lib/types';
import { PLATFORM_STREAM_SUPPORT } from '../lib/types';
import { PlatformBadge } from './PlatformBadge';
import { StatusBadge } from './StatusBadge';

interface Props {
  room: Room;
  onClick: (room: Room) => void;
  actionLabel?: string;
}

/** 观看人数格式化：1.2万 */
function formatViewers(n: number): string {
  if (n >= 10000) return `${(n / 10000).toFixed(1)}万`;
  return String(n);
}

export function LiveCard({ room, onClick, actionLabel }: Props) {
  const playable = PLATFORM_STREAM_SUPPORT[room.platform];
  const label = actionLabel ?? (playable ? '观看' : '外部打开');
  const firstChar = (room.username || room.title || '?').charAt(0);

  return (
    <div
      className="group cursor-pointer rounded-card border border-line bg-surface p-3 shadow-sm transition-all hover:-translate-y-0.5 hover:shadow-md"
      onClick={() => onClick(room)}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => e.key === 'Enter' && onClick(room)}
    >
      {/* 16:9 封面（接口提供时用真实封面，否则品牌渐变占位） */}
      <div className="relative flex aspect-video w-full items-center justify-center overflow-hidden rounded-lg bg-ink">
        {room.cover ? (
          <img
            src={room.cover}
            alt=""
            loading="lazy"
            className="absolute inset-0 h-full w-full object-cover"
            onError={(e) => {
              (e.target as HTMLImageElement).style.display = 'none';
            }}
          />
        ) : null}
        {!room.cover && (
          <div className="brand-gradient absolute inset-0 flex items-center justify-center opacity-90">
            <span className="text-4xl font-bold text-white/90">{firstChar}</span>
          </div>
        )}
        <div className="absolute left-2 top-2">
          <PlatformBadge platform={room.platform} size="sm" />
        </div>
        <div className="absolute right-2 top-2">
          <StatusBadge status={room.status} size="sm" />
        </div>
        {room.viewCount > 0 && (
          <div className="absolute bottom-2 right-2 rounded bg-black/60 px-1.5 py-0.5 text-[10px] text-white">
            {formatViewers(room.viewCount)}
          </div>
        )}
      </div>

      {/* 标题与主播 */}
      <h3 className="mt-2.5 line-clamp-1 text-sm font-semibold text-ink">{room.title}</h3>
      <div className="mt-1 flex items-center justify-between">
        <span className="line-clamp-1 text-xs text-slate">{room.username || '未知主播'}</span>
        <span className="text-xs font-medium text-stream opacity-0 transition-opacity group-hover:opacity-100">
          {label} →
        </span>
      </div>
    </div>
  );
}
