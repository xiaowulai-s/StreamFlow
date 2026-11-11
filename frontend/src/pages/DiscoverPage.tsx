// 直播发现：统一搜索框，跨平台并发搜索（虎牙/斗鱼/B 站），结果合并展示
// 平台语义：虎牙/斗鱼支持房间号+别名直达解析；B 站仅数字房间号
// 阶段④：B 站热门列表接入首页
import { useCallback, useEffect, useRef, useState } from 'react';
import { useApp } from '../context';
import { searchRooms } from '../lib/tauri';
import { type Platform, type Room } from '../lib/types';
import { LiveCard } from '../components/LiveCard';
import { EmptyState } from '../components/EmptyState';

/** 已验证可解析的平台（douyin/kuaishou 存根未接入，跳过） */
const SEARCH_PLATFORMS: Platform[] = ['huya', 'douyu', 'bilibili'];

export function DiscoverPage() {
  const { globalKeyword, setGlobalKeyword, watch } = useApp();
  const [results, setResults] = useState<Room[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const doSearch = useCallback(async (kw: string) => {
    const keyword = kw.trim();
    if (!keyword) return;
    setLoading(true);
    setError(null);

    // 跨平台并发搜索：命中的平台返回真实房间卡片，未命中的返回空/错误
    const settled = await Promise.allSettled(
      SEARCH_PLATFORMS.map((p) => searchRooms(p, keyword))
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

    if (rooms.length === 0 && errors.length === SEARCH_PLATFORMS.length) {
      // 全部平台失败（如 B 站拒收非数字关键词且其他平台解析失败）→ 展示原因
      setError(errors[0] ?? '搜索失败');
      setResults(null);
    } else {
      // 按 viewCount 降序，热门优先
      rooms.sort((a, b) => b.viewCount - a.viewCount);
      setResults(rooms);
    }
    setLoading(false);
  }, []);

  // 顶栏回车跳转过来时自动执行一次搜索（每个关键词只自动触发一次）
  const lastAutoSearch = useRef('');
  useEffect(() => {
    if (globalKeyword.trim() && lastAutoSearch.current !== globalKeyword) {
      lastAutoSearch.current = globalKeyword;
      doSearch(globalKeyword);
    }
  }, [globalKeyword, doSearch]);

  return (
    <div className="space-y-5">
      {/* 搜索区（跨平台统一搜索） */}
      <div className="rounded-card border border-line bg-surface p-4">
        <div className="flex gap-3">
          <input
            value={globalKeyword}
            onChange={(e) => setGlobalKeyword(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && doSearch(globalKeyword)}
            placeholder="输入房间号或主播别名（如 pigff、lpl、6），虎牙/斗鱼/B 站同时搜索"
            className="flex-1 rounded-lg border border-line bg-canvas px-4 py-2 text-sm text-ink outline-none focus:border-stream focus:bg-surface"
          />
          <button
            onClick={() => doSearch(globalKeyword)}
            disabled={loading || !globalKeyword.trim()}
            className="brand-gradient rounded-lg px-6 py-2 text-sm font-medium text-white transition-opacity hover:opacity-90 disabled:opacity-40"
          >
            {loading ? '搜索中…' : '全网搜索'}
          </button>
        </div>
        <p className="mt-2 text-[11px] text-slate">
          同时搜索虎牙 / 斗鱼 / B 站：虎牙、斗鱼支持房间号与主播别名直达（中文名搜索平台接口受限暂不支持）；B 站支持关键词、主播名与房间号。
        </p>
      </div>

      {/* 结果区 */}
      {error && <EmptyState title="无法解析该房间" description={error} />}
      {!error && !results && !loading && (
        <EmptyState
          title="输入关键词、房间号或主播别名开始"
          description="B 站支持关键词/主播名（如 悲喜）；虎牙、斗鱼请输入房间号或别名（如 lpl、pigff）"
        />
      )}
      {!error && !loading && results && results.length === 0 && (
        <EmptyState
          title="未找到相关直播间"
          description="虎牙/斗鱼暂不支持中文名搜索，请改用其房间号或主播别名；B 站关键词请确认拼写"
        />
      )}
      {!error && loading && (
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
      {!error && !loading && results && results.length > 0 && (
        <div>
          <p className="mb-3 text-xs text-slate">
            共 {results.length} 个结果，来自{' '}
            {Array.from(new Set(results.map((r) => r.platform))).length} 个平台
          </p>
          <div className="grid grid-cols-4 gap-4">
            {results.map((r) => (
              <LiveCard key={`${r.platform}-${r.id}`} room={r} onClick={watch} />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
