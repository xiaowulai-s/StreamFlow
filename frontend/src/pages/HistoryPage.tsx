// 观看历史：本地记录，按时间倒序；支持平台筛选（产品方案 5.8）
import { useMemo, useState } from 'react';
import { useApp } from '../context';
import { EmptyState } from '../components/EmptyState';
import { PlatformBadge } from '../components/PlatformBadge';
import { PLATFORM_LABELS, type Platform } from '../lib/types';

function formatTime(ts: number): string {
  const d = new Date(ts);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function HistoryPage() {
  const { history, watch, clearHistory } = useApp();
  const [platformFilter, setPlatformFilter] = useState<'all' | Platform>('all');

  // 实际出现过的平台（按出现次数排序）
  const platformCounts = useMemo(() => {
    const counts = new Map<Platform, number>();
    for (const h of history) counts.set(h.platform, (counts.get(h.platform) ?? 0) + 1);
    return Array.from(counts.entries()).sort((a, b) => b[1] - a[1]);
  }, [history]);

  const filtered = platformFilter === 'all' ? history : history.filter((h) => h.platform === platformFilter);

  return (
    <div className="space-y-5">
      <div className="flex items-center justify-between">
        <h1 className="text-lg font-bold text-ink">
          观看历史{' '}
          <span className="ml-1 text-sm font-normal text-slate">
            {platformFilter === 'all' ? `共 ${history.length} 条` : `${PLATFORM_LABELS[platformFilter]} ${filtered.length} 条`}
          </span>
        </h1>
        {history.length > 0 && (
          <button
            onClick={clearHistory}
            className="rounded-lg border border-line px-3 py-1.5 text-xs text-slate transition-colors hover:border-danger hover:text-danger"
          >
            清空历史
          </button>
        )}
      </div>

      {/* 平台筛选 */}
      {platformCounts.length > 1 && (
        <div className="flex flex-wrap items-center gap-2">
          {(['all', ...platformCounts.map(([p]) => p)] as ('all' | Platform)[]).map((p) => {
            const count = p === 'all' ? history.length : (platformCounts.find(([pp]) => pp === p)?.[1] ?? 0);
            return (
              <button
                key={p}
                onClick={() => setPlatformFilter(p)}
                className={`rounded-full border px-3 py-1 text-xs transition-colors ${
                  platformFilter === p
                    ? 'border-stream bg-stream/15 font-medium text-stream'
                    : 'border-line bg-surface text-slate hover:text-ink'
                }`}
              >
                {p === 'all' ? '全部' : PLATFORM_LABELS[p]}（{count}）
              </button>
            );
          })}
        </div>
      )}

      {filtered.length === 0 ? (
        <EmptyState title="暂无观看记录" description="打开过的直播间会自动记录在这里（仅保存在本机）" />
      ) : (
        <div className="space-y-2">
          {filtered.map((h) => (
            <button
              key={`${h.platform}-${h.id}-${h.openedAt}`}
              onClick={() => watch(h)}
              className="flex w-full items-center gap-4 rounded-card border border-line bg-surface px-5 py-3.5 text-left transition-shadow hover:shadow-md"
            >
              <PlatformBadge platform={h.platform} />
              <span className="min-w-0 flex-1 truncate text-sm font-medium text-ink">{h.title}</span>
              <span className="shrink-0 text-xs text-slate">{h.username || '未知主播'}</span>
              <span className="shrink-0 text-xs text-slate">{formatTime(h.openedAt)}</span>
              <span className="shrink-0 text-xs font-medium text-stream">继续观看 →</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
