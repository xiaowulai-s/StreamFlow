// 顶部工具栏：全局搜索 + 快捷统计 + 三平台登录状态（产品方案 4 / 5.9）
import { useApp } from '../context';
import { PlatformBadge } from './PlatformBadge';
import { SearchIcon, StarIcon, HistoryIcon, CloseIcon, SunIcon, MoonIcon } from './icons';
import type { Platform } from '../lib/types';

/** 展示登录状态的平台（含暂不支持登录的抖音） */
const AUTH_PLATFORMS: Platform[] = ['bilibili', 'douyu', 'huya', 'kuaishou', 'douyin'];

export function TopBar() {
  const {
    globalKeyword,
    setGlobalKeyword,
    navigate,
    favorites,
    history,
    page,
    authStatus,
    effectiveTheme,
    setTheme,
    hiddenFollowPlatforms,
  } = useApp();

  /** 展示中的关注数（与关注页/首页保持一致） */
  const visibleFavoriteCount = favorites.filter(
    (f) => !hiddenFollowPlatforms.has(f.platform),
  ).length;

  return (
    <header className="flex h-16 shrink-0 items-center gap-4 border-b border-line bg-surface px-6">
      {/* 全局搜索：回车回首页执行跨平台搜索 */}
      <div className="relative w-full max-w-md">
        <span className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-slate">
          <SearchIcon size={16} />
        </span>
        <input
          value={globalKeyword}
          onChange={(e) => setGlobalKeyword(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && globalKeyword.trim()) navigate('home');
            if (e.key === 'Escape') setGlobalKeyword('');
          }}
          placeholder="搜索房间号 / 主播名 / 关键词（回车全网搜索）"
          className="w-full rounded-lg border border-line bg-canvas py-2 pl-9 pr-9 text-sm text-ink outline-none transition-colors focus:border-stream focus:bg-surface"
        />
        {globalKeyword && (
          <button
            onClick={() => setGlobalKeyword('')}
            title="清除搜索"
            className="absolute right-2.5 top-1/2 flex h-5 w-5 -translate-y-1/2 items-center justify-center rounded-full text-slate transition-colors hover:bg-line hover:text-ink"
          >
            <CloseIcon size={12} />
          </button>
        )}
      </div>

      <div className="ml-auto flex items-center gap-3">
        {/* 快捷统计 */}
        <button
          onClick={() => navigate('favorites')}
          className={`flex items-center gap-1.5 text-xs transition-colors ${
            page === 'favorites' ? 'font-medium text-stream' : 'text-slate hover:text-ink'
          }`}
          title="已关注主播数"
        >
          <StarIcon size={15} />
          {visibleFavoriteCount}
        </button>
        <button
          onClick={() => navigate('history')}
          className={`flex items-center gap-1.5 text-xs transition-colors ${
            page === 'history' ? 'font-medium text-stream' : 'text-slate hover:text-ink'
          }`}
          title="历史记录数"
        >
          <HistoryIcon size={15} />
          {history.length}
        </button>

        <span className="h-5 w-px bg-line" />

        {/* 三平台登录状态：已登录高亮 + 绿点；未登录灰度；点击进设置 */}
        <div className="flex items-center gap-1">
          {AUTH_PLATFORMS.map((platform) => {
            const session = authStatus?.find((s) => s.platform === platform);
            const loggedIn = session?.loggedIn ?? false;
            const label = session?.label ?? platform;
            const canLogin = session?.canLogin ?? true;
            const tip = loggedIn
              ? `${label}：已登录${session?.username ? `（${session.username}）` : ''}`
              : canLogin
                ? `${label}：未登录，点击去设置中心登录`
                : `${label}：暂不支持登录（弹幕/观看可正常使用）`;
            return (
              <button
                key={platform}
                onClick={() => navigate('settings')}
                title={tip}
                aria-label={tip}
                className={`relative rounded-md p-0.5 transition-all ${
                  loggedIn
                    ? 'opacity-100 hover:-translate-y-0.5'
                    : 'opacity-40 grayscale hover:opacity-70'
                }`}
              >
                <PlatformBadge platform={platform} size="sm" />
                <span
                  className={`absolute -right-0.5 -top-0.5 h-2 w-2 rounded-full border-2 border-surface ${
                    loggedIn ? 'bg-success' : 'bg-slate/40'
                  }`}
                />
              </button>
            );
          })}
        </div>

        {/* 主题快捷切换：浅色 ↔ 深色（跟随系统时切换为显式主题） */}
        <button
          onClick={() => setTheme(effectiveTheme === 'dark' ? 'light' : 'dark')}
          title={effectiveTheme === 'dark' ? '切换到浅色主题' : '切换到深色主题'}
          aria-label="切换主题"
          className="flex h-8 w-8 items-center justify-center rounded-lg border border-line text-slate transition-colors hover:border-stream hover:text-stream"
        >
          {effectiveTheme === 'dark' ? <SunIcon size={16} /> : <MoonIcon size={16} />}
        </button>
      </div>
    </header>
  );
}
