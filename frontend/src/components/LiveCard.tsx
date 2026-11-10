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
      {/* 16:9 封面占位 */}
      <div className="brand-gradient relative flex aspect-video w-full items-center justify-center overflow-hidden rounded-lg opacity-90">
        <span className="text-4xl font-bold text-white/90">{firstChar}</span>
        <div className="absolute left-2 top-2">
          <PlatformBadge platform={room.platform} size="sm" />
        </div>
        <div className="absolute right-2 top-2">
          <StatusBadge status={room.status} size="sm" />
        </div>
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
