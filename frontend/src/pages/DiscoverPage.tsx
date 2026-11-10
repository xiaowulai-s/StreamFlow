// 直播发现：平台 + 关键词搜索，空结果生成房间号直达卡片（产品方案 5.3）
// 阶段③ 将在此接入别名解析；阶段④ 接入 B 站热门列表
import { useCallback, useEffect, useRef, useState } from 'react';
import { useApp } from '../context';
import { searchRooms } from '../lib/tauri';
import { PLATFORM_LABELS, PLATFORM_STREAM_SUPPORT, type Platform, type Room } from '../lib/types';
import { LiveCard } from '../components/LiveCard';
import { EmptyState } from '../components/EmptyState';

const PLATFORMS: Platform[] = ['bilibili', 'douyu', 'huya', 'douyin', 'kuaishou'];

export function DiscoverPage() {
  const { globalKeyword, setGlobalKeyword, watch } = useApp();
  const [platform, setPlatform] = useState<Platform>('bilibili');
  const [results, setResults] = useState<Room[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const doSearch = useCallback(
    async (kw: string) => {
      const keyword = kw.trim();
      if (!keyword) return;
      setLoading(true);
      setError(null);
      try {
        // 后端语义：虎牙/斗鱼支持房间号+别名直达解析，B 站仅数字房间号
        // 解析成功返回真实房间卡片；失败返回空列表或带原因的错误
        const list = await searchRooms(platform, keyword);
        setResults(Array.isArray(list) ? list : []);
      } catch (err) {
        setError(String(err));
        setResults(null);
      } finally {
        setLoading(false);
      }
    },
    [platform]
  );

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
      {/* 搜索区 */}
      <div className="rounded-card border border-line bg-surface p-4">
        <div className="flex gap-3">
          <select
            value={platform}
            onChange={(e) => {
              setPlatform(e.target.value as Platform);
              setResults(null);
            }}
            className="rounded-lg border border-line bg-canvas px-3 py-2 text-sm font-medium text-ink outline-none focus:border-stream"
          >
            {PLATFORMS.map((p) => (
              <option key={p} value={p}>
                {PLATFORM_LABELS[p]}
                {PLATFORM_STREAM_SUPPORT[p] ? '' : '（未验证）'}
              </option>
            ))}
          </select>
          <input
            value={globalKeyword}
            onChange={(e) => setGlobalKeyword(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && doSearch(globalKeyword)}
            placeholder="输入房间号或关键词（虎牙/斗鱼支持别名，B 站仅数字房间号）"
            className="flex-1 rounded-lg border border-line bg-canvas px-4 py-2 text-sm text-ink outline-none focus:border-stream focus:bg-surface"
          />
          <button
            onClick={() => doSearch(globalKeyword)}
            disabled={loading || !globalKeyword.trim()}
            className="rounded-lg bg-stream px-6 py-2 text-sm font-medium text-white transition-opacity hover:opacity-90 disabled:opacity-40"
          >
            {loading ? '搜索中…' : '搜索'}
          </button>
        </div>
        <p className="mt-2 text-[11px] text-slate">
          三平台关键词搜索接口均受平台限制：虎牙/斗鱼已下线、B 站需登录风控。输入房间号（虎牙/斗鱼支持别名）即可直达观看。
        </p>
      </div>

      {/* 结果区 */}
      {error && (
        <EmptyState title="无法解析该房间" description={error.replace(/^.*?错误信息[：:]?/, '')} />
      )}
      {!error && !results && !loading && (
        <EmptyState
          title="输入房间号或别名开始"
          description="虎牙 / 斗鱼支持主播别名直达（如 pigff、lpl）；B 站请输入数字房间号（如 6）"
        />
      )}
      {!error && !loading && results && results.length === 0 && (
        <EmptyState
          title="未找到该直播间"
          description="请确认房间号/别名是否正确；房间可能已注销或未开放"
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
      {!error && results && results.length > 0 && (
        <div className="grid grid-cols-4 gap-4">
          {results.map((r) => (
            <LiveCard key={`${r.platform}-${r.id}`} room={r} onClick={watch} />
          ))}
        </div>
      )}
    </div>
  );
}
