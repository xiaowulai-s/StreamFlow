// 顶部工具栏：全局搜索 + 状态统计（产品方案 4）
import { useApp } from '../context';
import { SearchIcon, StarIcon, HistoryIcon } from './icons';

export function TopBar() {
  const { globalKeyword, setGlobalKeyword, navigate, favorites, history, page } = useApp();

  const titles: Record<string, string> = {
    home: '首页',
    discover: '直播发现',
    watch: '直播观看',
    favorites: '我的关注',
    history: '观看历史',
  };

  return (
    <header className="flex h-16 shrink-0 items-center gap-4 border-b border-line bg-surface px-6">
      {/* 全局搜索：跳转发现页执行 */}
      <div className="relative w-full max-w-md">
        <span className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-slate">
          <SearchIcon size={16} />
        </span>
        <input
          value={globalKeyword}
          onChange={(e) => setGlobalKeyword(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && globalKeyword.trim()) navigate('discover');
          }}
          placeholder="搜索房间号 / 主播别名（回车跳转发现页）"
          className="w-full rounded-lg border border-line bg-canvas py-2 pl-9 pr-3 text-sm text-ink outline-none transition-colors focus:border-stream focus:bg-surface"
        />
      </div>

      <div className="ml-auto flex items-center gap-4">
        {/* 快捷统计 */}
        <button
          onClick={() => navigate('favorites')}
          className={`flex items-center gap-1.5 text-xs transition-colors ${
            page === 'favorites' ? 'font-medium text-stream' : 'text-slate hover:text-ink'
          }`}
          title="已关注主播数"
        >
          <StarIcon size={15} />
          {favorites.length}
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
        {/* 窗口控制区占位（系统标题栏仍由 Tauri 装饰） */}
        <span className="rounded-card bg-canvas px-3 py-1.5 text-[10px] text-slate" title="当前页面">
          {titles[page] ?? ''}
        </span>
      </div>
    </header>
  );
}
