// 我的关注：跨平台收藏列表 + 平台关注导入（产品方案 5.7）
import { useState } from 'react';
import { useApp } from '../context';
import { PLATFORM_LABELS, type Platform, type Room } from '../lib/types';
import { getAuthStatus, getPlatformFollows } from '../lib/tauri';
import { LiveCard } from '../components/LiveCard';
import { EmptyState } from '../components/EmptyState';
import { CloseIcon } from '../components/icons';

/** 支持关注列表导入的平台（B站/斗鱼/快手走 API，虎牙解析订阅页） */
const IMPORT_PLATFORMS = new Set(['bilibili', 'douyu', 'huya', 'kuaishou']);

export function FavoritesPage() {
  const {
    favorites,
    watch,
    toggleFavorite,
    navigate,
    mergeFavorites,
    hiddenFollowPlatforms,
    toggleFollowVisible,
  } = useApp();
  const [syncing, setSyncing] = useState(false);
  const [syncMessage, setSyncMessage] = useState<string | null>(null);

  // 按平台分组（产品方案：按平台和自定义分组管理，首版按平台）
  const groups = new Map<Platform, typeof favorites>();
  for (const f of favorites) {
    const list = groups.get(f.platform) ?? [];
    list.push(f);
    groups.set(f.platform, list);
  }

  /** 展示中的分组（用户在分组头用眼睛图标控制隐藏/显示） */
  const visibleGroups = Array.from(groups.entries()).filter(
    ([platform]) => !hiddenFollowPlatforms.has(platform),
  );
  const visibleCount = visibleGroups.reduce((n, [, items]) => n + items.length, 0);

  const handleSync = async () => {
    setSyncing(true);
    setSyncMessage(null);
    try {
      const auth = await getAuthStatus();
      const targets = auth.filter((s) => s.loggedIn && IMPORT_PLATFORMS.has(s.platform));
      if (targets.length === 0) {
        setSyncMessage('请先在设置中心登录 B站、斗鱼 或 快手，再同步关注列表');
        return;
      }
      const collected: Room[] = [];
      const parts: string[] = [];
      for (const target of targets) {
        try {
          const rooms = await getPlatformFollows(target.platform);
          collected.push(...rooms);
          parts.push(`${target.label} ${rooms.length} 个`);
        } catch (e) {
          parts.push(`${target.label} 同步失败（${String(e)}）`);
        }
      }
      const added = mergeFavorites(collected);
      setSyncMessage(`同步完成：${parts.join('、')}；本次新增 ${added} 位关注`);
    } catch (e) {
      setSyncMessage(String(e));
    } finally {
      setSyncing(false);
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between gap-4">
        <h1 className="text-lg font-bold text-ink">
          我的关注 <span className="ml-1 text-sm font-normal text-slate">共 {visibleCount} 位</span>
        </h1>
        <button
          onClick={handleSync}
          disabled={syncing}
          className="brand-gradient rounded-lg px-4 py-2 text-xs font-medium text-white transition-opacity hover:opacity-90 disabled:opacity-40"
        >
          {syncing ? '同步中…' : '同步平台关注'}
        </button>
      </div>

      {syncMessage && (
        <div className="rounded-lg border border-line bg-surface px-4 py-3 text-xs text-slate">
          {syncMessage}
        </div>
      )}

      {favorites.length === 0 ? (
        <EmptyState
          title="还没有收藏的主播"
          description="进入直播间后点击「收藏主播」，或在设置中心登录平台后点右上角「同步平台关注」导入"
          action={{ label: '回首页', onClick: () => navigate('home') }}
        />
      ) : (
        Array.from(groups.entries()).map(([platform, items]) => {
          const hidden = hiddenFollowPlatforms.has(platform);
          return (
          <section key={platform}>
            <div className="mb-3 flex items-center gap-2">
              <h2 className="text-sm font-bold text-slate">
                {PLATFORM_LABELS[platform]}（{items.length}）
              </h2>
              {hidden && (
                <span className="rounded border border-line px-1.5 py-0.5 text-[10px] text-slate">
                  已隐藏
                </span>
              )}
              <button
                onClick={() => toggleFollowVisible(platform)}
                title={hidden ? `展示${PLATFORM_LABELS[platform]}关注` : `隐藏${PLATFORM_LABELS[platform]}关注`}
                className={`rounded px-2 py-0.5 text-[10px] transition-colors ${
                  hidden
                    ? 'border border-line text-slate hover:border-stream hover:text-stream'
                    : 'text-slate hover:text-ink'
                }`}
              >
                {hidden ? '显示' : '隐藏'}
              </button>
            </div>
            {!hidden && (
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
            )}
          </section>
          );
        })
      )}
    </div>
  );
}
