// 首页：关注开播条 + 平台快捷入口 + 最近观看（产品方案 5.2）
import { useApp } from '../context';
import { PLATFORM_LABELS, PLATFORM_STREAM_SUPPORT, type Platform } from '../lib/types';
import { LiveCard } from '../components/LiveCard';
import { EmptyState } from '../components/EmptyState';
import { StatusBadge } from '../components/StatusBadge';

const PLATFORMS: Platform[] = ['bilibili', 'douyu', 'huya', 'douyin', 'kuaishou'];

export function HomePage() {
  const { favorites, history, navigate, watch, setGlobalKeyword } = useApp();
  const liveFavorites = favorites.filter((f) => f.status === 'live');
  const recent = history.slice(0, 4);

  return (
    <div className="space-y-6">
      {/* 关注开播条 */}
      <section>
        <div className="mb-3 flex items-center justify-between">
          <h2 className="text-base font-bold text-ink">关注开播</h2>
          {favorites.length > 0 && (
            <button onClick={() => navigate('favorites')} className="text-xs text-stream hover:underline">
              全部关注（{favorites.length}）→
            </button>
          )}
        </div>
        {liveFavorites.length === 0 ? (
          <div className="rounded-card border border-line bg-surface px-5 py-4 text-sm text-slate">
            {favorites.length === 0
              ? '还没有关注的主播——在直播间里点击「收藏」后，开播状态会显示在这里'
              : `已关注 ${favorites.length} 位主播，当前均未开播`}
          </div>
        ) : (
          <div className="space-y-2">
            {liveFavorites.map((f) => (
              <button
                key={`${f.platform}-${f.id}`}
                onClick={() => watch(f)}
                className="flex w-full items-center gap-3 rounded-card border border-line bg-surface px-4 py-3 text-left transition-shadow hover:shadow-md"
              >
                <StatusBadge status="live" size="sm" />
                <span className="min-w-0 flex-1 truncate text-sm font-medium text-ink">{f.title}</span>
                <span className="shrink-0 text-xs text-slate">{f.username}</span>
                <span className="shrink-0 text-xs text-stream">{PLATFORM_LABELS[f.platform]} →</span>
              </button>
            ))}
          </div>
        )}
      </section>

      {/* 平台快捷入口 */}
      <section>
        <h2 className="mb-3 text-base font-bold text-ink">平台</h2>
        <div className="grid grid-cols-5 gap-3">
          {PLATFORMS.map((p) => (
            <button
              key={p}
              onClick={() => {
                setGlobalKeyword('');
                navigate('discover');
              }}
              className="rounded-card border border-line bg-surface p-4 text-left transition-all hover:-translate-y-0.5 hover:shadow-md"
            >
              <div className="text-lg font-bold text-ink">{PLATFORM_LABELS[p]}</div>
              <div className="mt-1 text-[11px] text-slate">
                {PLATFORM_STREAM_SUPPORT[p] ? '站内播放已验证' : '待接入验证'}
              </div>
            </button>
          ))}
        </div>
      </section>

      {/* 最近观看 */}
      <section>
        <div className="mb-3 flex items-center justify-between">
          <h2 className="text-base font-bold text-ink">最近观看</h2>
          {history.length > 0 && (
            <button onClick={() => navigate('history')} className="text-xs text-stream hover:underline">
              全部历史（{history.length}）→
            </button>
          )}
        </div>
        {recent.length === 0 ? (
          <EmptyState
            title="暂无观看记录"
            description="去「直播发现」用房间号直达直播间，或等热门直播接入后直接挑选"
            action={{ label: '去发现', onClick: () => navigate('discover') }}
          />
        ) : (
          <div className="grid grid-cols-4 gap-4">
            {recent.map((h) => (
              <LiveCard key={`${h.platform}-${h.id}-${h.openedAt}`} room={h} onClick={watch} />
            ))}
          </div>
        )}
      </section>
    </div>
  );
}
