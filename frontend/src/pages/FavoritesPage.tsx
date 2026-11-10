// 我的关注：跨平台收藏列表（产品方案 5.7，首版不含开播提醒）
import { useApp } from '../context';
import { PLATFORM_LABELS, type Platform } from '../lib/types';
import { LiveCard } from '../components/LiveCard';
import { EmptyState } from '../components/EmptyState';
import { CloseIcon } from '../components/icons';

export function FavoritesPage() {
  const { favorites, watch, toggleFavorite, navigate } = useApp();

  // 按平台分组（产品方案：按平台和自定义分组管理，首版按平台）
  const groups = new Map<Platform, typeof favorites>();
  for (const f of favorites) {
    const list = groups.get(f.platform) ?? [];
    list.push(f);
    groups.set(f.platform, list);
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-lg font-bold text-ink">
          我的关注 <span className="ml-1 text-sm font-normal text-slate">共 {favorites.length} 位</span>
        </h1>
      </div>

      {favorites.length === 0 ? (
        <EmptyState
          title="还没有收藏的主播"
          description="进入直播间后点击「收藏主播」，即可在这里跟踪开播状态"
          action={{ label: '去发现', onClick: () => navigate('discover') }}
        />
      ) : (
        Array.from(groups.entries()).map(([platform, items]) => (
          <section key={platform}>
            <h2 className="mb-3 text-sm font-bold text-slate">
              {PLATFORM_LABELS[platform]}（{items.length}）
            </h2>
            <div className="grid grid-cols-4 gap-4">
              {items.map((f) => (
                <div key={`${f.platform}-${f.id}`} className="relative">
                  <LiveCard room={f} onClick={watch} />
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      toggleFavorite(f);
                    }}
                    title="取消关注"
                    className="absolute -right-2 -top-2 flex h-6 w-6 items-center justify-center rounded-full border border-line bg-surface text-slate shadow-sm transition-colors hover:border-danger hover:text-danger"
                  >
                    <CloseIcon size={13} />
                  </button>
                </div>
              ))}
            </div>
          </section>
        ))
      )}
    </div>
  );
}
