// 左侧导航（产品方案 4：固定 208-232px，可折叠为图标栏）
import { useState, type ReactElement } from 'react';
import { useApp, type Page } from '../context';
import {
  LogoMark,
  HomeIcon,
  StarIcon,
  HistoryIcon,
  PlayIcon,
  SettingsIcon,
} from './icons';

const NAV_ITEMS: { page: Page; label: string; icon: (p: { size?: number }) => ReactElement }[] = [
  { page: 'home', label: '首页', icon: HomeIcon },
  { page: 'favorites', label: '我的关注', icon: StarIcon },
  { page: 'history', label: '观看历史', icon: HistoryIcon },
  { page: 'settings', label: '设置中心', icon: SettingsIcon },
];

export function SideNav() {
  const { page, navigate, watching } = useApp();
  const [collapsed, setCollapsed] = useState(false);

  return (
    <aside
      className={`flex shrink-0 flex-col border-r border-line bg-surface transition-all duration-200 ${
        collapsed ? 'w-16' : 'w-56'
      }`}
    >
      {/* 品牌（产品方案 2.3：独立图形标识，安全空间） */}
      <div className="flex h-16 items-center gap-2.5 px-4">
        <LogoMark size={34} />
        {!collapsed && (
          <div className="min-w-0">
            <div className="truncate text-sm font-bold text-ink">Z</div>
            <div className="truncate text-[10px] text-slate"></div>
          </div>
        )}
      </div>

      {/* 导航项 */}
      <nav className="mt-2 flex-1 px-2.5">
        {NAV_ITEMS.map((item) => {
          const active = page === item.page;
          const Icon = item.icon;
          return (
            <button
              key={item.page}
              onClick={() => navigate(item.page)}
              title={item.label}
              className={`mb-1 flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-sm transition-colors ${
                active
                  ? 'bg-stream/10 font-semibold text-stream'
                  : 'text-slate hover:bg-canvas hover:text-ink'
              }`}
            >
              <Icon size={18} />
              {!collapsed && <span className="truncate">{item.label}</span>}
            </button>
          );
        })}

        {/* 观看中（打开直播间后出现） */}
        {watching && (
          <button
            onClick={() => navigate('watch')}
            title={`观看中：${watching.title}`}
            className={`mt-1 flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-sm transition-colors ${
              page === 'watch'
                ? 'bg-stream/10 font-semibold text-stream'
                : 'text-slate hover:bg-canvas hover:text-ink'
            }`}
          >
            <PlayIcon size={18} />
            {!collapsed && <span className="truncate">观看中</span>}
            {page === 'watch' && (
              <span className="ml-auto h-2 w-2 shrink-0 rounded-full bg-success animate-pulse" />
            )}
          </button>
        )}
      </nav>

      {/* 折叠按钮 */}
      <button
        onClick={() => setCollapsed((c) => !c)}
        className="m-2.5 rounded-lg border border-line py-2 text-xs text-slate transition-colors hover:bg-canvas hover:text-ink"
        title={collapsed ? '展开导航' : '折叠导航'}
      >
        {collapsed ? '»' : '« 折叠'}
      </button>
    </aside>
  );
}
