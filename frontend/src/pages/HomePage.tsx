// 首页：关注开播 + 最近观看（搜索与热门由顶部搜索/平台页承担）
import { useCallback, useEffect, useRef, useState } from 'react';
import { useApp } from '../context';
import { openExternal, getSetting, searchRooms } from '../lib/tauri';
import { PLATFORM_LABELS, platformRoomUrl, type Platform, type Room } from '../lib/types';
import { LiveCard } from '../components/LiveCard';
import { EmptyState } from '../components/EmptyState';
import { StatusBadge } from '../components/StatusBadge';

/** 已验证可搜索的平台 */
const SEARCH_PLATFORMS: Platform[] = ['huya', 'douyu', 'bilibili', 'douyin', 'kuaishou'];

type PlatformFilter = 'all' | Platform;
type SortMode = 'hot' | 'latest';

export function HomePage() {
  const { favorites, history, navigate, watch, globalKeyword, setGlobalKeyword, hiddenFollowPlatforms, disabledPlatforms } = useApp();
  const keyword = globalKeyword.trim();
  const searching = keyword.length > 0;

  /** 展示中的关注（用户可在关注页/设置中心按平台隐藏） */
  const shownFavorites = favorites.filter((f) => !hiddenFollowPlatforms.has(f.platform));
  const liveFavorites = shownFavorites.filter((f) => f.status === 'live');
  const recent = history.slice(0, 4);

  // 搜索结果
  const [results, setResults] = useState<Room[] | null>(null);
  const [searchLoading, setSearchLoading] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [filter, setFilter] = useState<PlatformFilter>('all');
  const [sort, setSort] = useState<SortMode>('hot');
  const lastSearch = useRef('');

  // 搜索范围（6.x 用户需求）：all=全站，或只搜选定平台；变化时自动重搜当前关键词
  const [searchScope, setSearchScope] = useState<'all' | Platform>('all');

  const doSearch = useCallback(async (kw: string) => {
    setSearchLoading(true);
    setSearchError(null);
    // 范围：指定平台 or 已启用平台（6.1 平台管理）
    const enabled = SEARCH_PLATFORMS.filter((p) => !disabledPlatforms.has(p));
    const targets = (searchScope === 'all' ? enabled : [searchScope]).filter((p) =>
      enabled.includes(p),
    );
    const settled = await Promise.allSettled(
      (targets.length > 0 ? targets : SEARCH_PLATFORMS).map((p) => searchRooms(p, kw))
    );
    const rooms: Room[] = [];
    const errors: string[] = [];
    for (const r of settled) {
      if (r.status === 'fulfilled' && Array.isArray(r.value)) {
        rooms.push(...r.value);
      } else if (r.status === 'rejected') {
        errors.push(String(r.reason));
      }
    }
    if (rooms.length === 0 && errors.length === targets.length) {
      setSearchError(errors[0] ?? '搜索失败');
      setResults(null);
    } else {
      setResults(rooms);
    }
    setSearchLoading(false);
  }, [disabledPlatforms, searchScope]);

  // 关键词变化 → 执行搜索；清空 → 回到浏览态
  useEffect(() => {
    if (!keyword) {
      setResults(null);
      setSearchError(null);
      setFilter('all');
      setSort('hot');
      lastSearch.current = '';
      return;
    }
    if (lastSearch.current === keyword) return;
    lastSearch.current = keyword;
    doSearch(keyword);
  }, [keyword, doSearch]);

  const hasStartedAt = (results ?? []).some((r) => typeof r.startedAt === 'number');
  useEffect(() => {
    if (!hasStartedAt && sort !== 'hot') setSort('hot');
  }, [hasStartedAt, sort]);

  // 默认打开方式（6.2）：external 时卡片点击直接调系统浏览器，不进观看页
  const [openMode, setOpenMode] = useState<'internal' | 'external'>('internal');
  // 按平台覆盖（6.5 扩展）：观看页"外部打开"时记忆该平台偏好
  const [platformModes, setPlatformModes] = useState<Record<string, 'internal' | 'external'>>({});
  useEffect(() => {
    void getSetting('default_open_mode')
      .then((v) => {
        if (v === 'internal' || v === 'external') setOpenMode(v);
      })
      .catch(() => undefined);
    // 读取各平台覆盖（存为 JSON：{douyin:"external",...}）
    void getSetting('platform_open_modes')
      .then((v) => {
        if (v) {
          try {
            const obj = JSON.parse(v);
            if (obj && typeof obj === 'object') setPlatformModes(obj);
          } catch {
            // 忽略损坏数据
          }
        }
      })
      .catch(() => undefined);
  }, []);
  const handleCardClick = useCallback(
    (room: Room) => {
      // 平台覆盖优先，其次全局默认
      const mode = platformModes[room.platform] ?? openMode;
      if (mode === 'external') {
        void openExternal(platformRoomUrl(room.platform, room.id)).catch(() => undefined);
        return;
      }
      watch(room);
    },
    [openMode, platformModes, watch],
  );

  const filtered =
    results && filter !== 'all' ? results.filter((r) => r.platform === filter) : results;
  const sorted = filtered
    ? [...filtered].sort((a, b) =>
        sort === 'hot'
          ? b.viewCount - a.viewCount
          : (b.startedAt ?? 0) - (a.startedAt ?? 0),
      )
    : null;

  // ---------------- 搜索态 ----------------
  if (searching) {
    return (
      <div className="space-y-5">
        <div className="flex items-start justify-between gap-4">
          <div>
            <h1 className="text-lg font-bold text-ink">搜索「{keyword}」</h1>
            <p className="mt-1 text-xs text-slate">
              跨平台搜索虎牙 / 斗鱼 / B站 / 抖音 / 快手；修改顶部关键词后回车可重新搜索
            </p>
          </div>
          <label className="flex shrink-0 items-center gap-2 text-xs text-slate">
            搜索范围
            <select
              value={searchScope}
              onChange={(e) => setSearchScope(e.target.value as 'all' | Platform)}
              className="rounded-lg border border-line bg-surface px-2 py-1.5 text-xs text-ink"
            >
              <option value="all">全站</option>
              {SEARCH_PLATFORMS.filter((p) => !disabledPlatforms.has(p)).map((p) => (
                <option key={p} value={p}>
                  {PLATFORM_LABELS[p]}
                </option>
              ))}
            </select>
          </label>
          <button
            onClick={() => setGlobalKeyword('')}
            className="shrink-0 rounded-lg border border-line px-3 py-1.5 text-xs text-slate transition-colors hover:border-stream hover:text-stream"
          >
            清除搜索
          </button>
        </div>

        {results && results.length > 0 && (
          <div className="flex flex-wrap items-center gap-2">
            {(['all', ...SEARCH_PLATFORMS] as PlatformFilter[]).map((p) => {
              const count =
                p === 'all' ? results.length : results.filter((r) => r.platform === p).length;
              if (p !== 'all' && count === 0) return null;
              return (
                <button
                  key={p}
                  onClick={() => setFilter(p)}
                  className={`rounded-full border px-3 py-1 text-xs transition-colors ${
                    filter === p
                      ? 'border-stream bg-stream/15 font-medium text-stream'
                      : 'border-line bg-surface text-slate hover:text-ink'
                  }`}
                >
                  {p === 'all' ? '全部' : PLATFORM_LABELS[p]}（{count}）
                </button>
              );
            })}
            <span className="mx-1 h-4 w-px bg-line" />
            {(['hot', 'latest'] as SortMode[]).map((mode) => {
              if (mode === 'latest' && !hasStartedAt) return null;
              return (
                <button
                  key={mode}
                  onClick={() => setSort(mode)}
                  className={`rounded-full border px-3 py-1 text-xs transition-colors ${
                    sort === mode
                      ? 'border-stream bg-stream/10 text-stream'
                      : 'border-line bg-surface text-slate hover:text-ink'
                  }`}
                >
                  {mode === 'hot' ? '热度' : '最新开播'}
                </button>
              );
            })}
          </div>
        )}

        {searchError && <EmptyState title="搜索失败" description={searchError} />}
        {!searchError && (searchLoading || (!results && !searchError)) && (
          <div className="grid grid-cols-4 gap-4">
            {Array.from({ length: 4 }).map((_, i) => (
              <div key={i} className="animate-pulse rounded-card border border-line bg-surface p-3">
                <div className="aspect-video rounded-lg bg-canvas" />
                <div className="mt-2.5 h-4 w-3/4 rounded bg-canvas" />
                <div className="mt-2 h-3 w-1/2 rounded bg-canvas" />
              </div>
            ))}
          </div>
        )}
        {!searchError && !searchLoading && results && results.length === 0 && (
          <EmptyState
            title="未找到相关直播间"
            description="请尝试其他关键词，或直接输入房间号直达"
          />
        )}
        {!searchError && !searchLoading && sorted && sorted.length > 0 && (
          <div className="grid grid-cols-4 gap-4">
            {sorted.map((r) => (
              <LiveCard key={`${r.platform}-${r.id}`} room={r} onClick={handleCardClick} />
            ))}
          </div>
        )}
      </div>
    );
  }

  // ---------------- 浏览态 ----------------
  return (
    <div className="space-y-6">
      {/* 关注开播条 */}
      <section>
        <div className="mb-3 flex items-center justify-between">
          <h2 className="text-base font-bold text-ink">关注开播</h2>
          {shownFavorites.length > 0 && (
            <button onClick={() => navigate('favorites')} className="text-xs text-stream hover:underline">
              全部关注（{shownFavorites.length}）→
            </button>
          )}
        </div>
        {liveFavorites.length === 0 ? (
          <div className="rounded-card border border-line bg-surface px-5 py-4 text-sm text-slate">
            {shownFavorites.length === 0
              ? '还没有关注的主播——在直播间里点击「收藏」后，开播状态会显示在这里'
              : `已关注 ${shownFavorites.length} 位主播，当前均未开播`}
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
            description="在顶部搜索框输入房间号 / 主播名 / 关键词开始观看"
          />
        ) : (
          <div className="grid grid-cols-4 gap-4">
            {recent.map((h) => (
              <LiveCard key={`${h.platform}-${h.id}-${h.openedAt}`} room={h} onClick={handleCardClick} />
            ))}
          </div>
        )}
      </section>
    </div>
  );
}
