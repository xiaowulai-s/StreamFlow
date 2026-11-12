// 应用布局壳：左侧导航 + 顶部工具栏 + 页面路由（产品方案 4 信息架构）
import { useCallback, useEffect, useState } from 'react';
import { AppContext, type Page } from './context';
import { useFavorites, useHistory } from './lib/storage';
import { getAuthStatus, getSetting, setSetting, type AuthStatus } from './lib/tauri';
import { useTheme } from './lib/theme';
import type { Room } from './lib/types';
import { SideNav } from './components/SideNav';
import { TopBar } from './components/TopBar';
import { HomePage } from './pages/HomePage';
import { WatchPage } from './pages/WatchPage';
import { FavoritesPage } from './pages/FavoritesPage';
import { HistoryPage } from './pages/HistoryPage';
import { SettingsPage } from './pages/SettingsPage';

/** app_settings 中存储隐藏平台 JSON 数组的 key */
const HIDDEN_FOLLOW_KEY = 'hidden_follow_platforms';
/** app_settings 中存储停用平台 JSON 数组的 key */
const DISABLED_PLATFORMS_KEY = 'disabled_platforms';

export default function App() {
  const [page, setPage] = useState<Page>('home');
  const [watchRoom, setWatchRoom] = useState<Room | null>(null);
  const [globalKeyword, setGlobalKeyword] = useState('');
  const [authStatus, setAuthStatus] = useState<AuthStatus[] | null>(null);
  const [hiddenFollowPlatforms, setHiddenFollowPlatforms] = useState<Set<string>>(new Set());
  const [disabledPlatforms, setDisabledPlatforms] = useState<Set<string>>(new Set());

  // 平台关注展示开关：启动时读取持久化设置
  useEffect(() => {
    getSetting(HIDDEN_FOLLOW_KEY)
      .then((v) => {
        if (v) {
          try {
            const arr: unknown = JSON.parse(v);
            if (Array.isArray(arr)) setHiddenFollowPlatforms(new Set(arr.map(String)));
          } catch {
            // 损坏数据按默认（全部展示）处理
          }
        }
      })
      .catch(() => undefined);
    getSetting(DISABLED_PLATFORMS_KEY)
      .then((v) => {
        if (v) {
          try {
            const arr: unknown = JSON.parse(v);
            if (Array.isArray(arr)) setDisabledPlatforms(new Set(arr.map(String)));
          } catch {
            // 损坏数据按默认（全部启用）处理
          }
        }
      })
      .catch(() => undefined);
  }, []);

  const toggleFollowVisible = useCallback((platform: string) => {
    setHiddenFollowPlatforms((prev) => {
      const next = new Set(prev);
      if (next.has(platform)) {
        next.delete(platform);
      } else {
        next.add(platform);
      }
      void setSetting(HIDDEN_FOLLOW_KEY, JSON.stringify([...next])).catch(() => undefined);
      return next;
    });
  }, []);

  const togglePlatformEnabled = useCallback((platform: string) => {
    setDisabledPlatforms((prev) => {
      const next = new Set(prev);
      if (next.has(platform)) {
        next.delete(platform);
      } else {
        next.add(platform);
      }
      void setSetting(DISABLED_PLATFORMS_KEY, JSON.stringify([...next])).catch(() => undefined);
      return next;
    });
  }, []);

  const { favorites, isFavorite, toggleFavorite, mergeFavorites } = useFavorites();
  const { history, addHistory, clearHistory, applyRetention } = useHistory();
  const { pref: theme, effective: effectiveTheme, setPref: setTheme } = useTheme();

  // 三平台登录状态：启动拉取，设置页登录/退出后刷新
  const refreshAuth = useCallback(() => {
    getAuthStatus()
      .then(setAuthStatus)
      .catch(() => setAuthStatus([]));
  }, []);

  useEffect(() => {
    refreshAuth();
  }, [refreshAuth]);

  // 从任意卡片进入观看页：切换页面 + 写入观看历史
  const watch = (room: Room) => {
    setWatchRoom(room);
    addHistory(room);
    setPage('watch');
  };

  return (
    <AppContext.Provider
      value={{
        page,
        navigate: setPage,
        watching: watchRoom,
        watch,
        globalKeyword,
        setGlobalKeyword,
        favorites,
        isFavorite,
        toggleFavorite,
        mergeFavorites,
        history,
        clearHistory,
        applyRetention,
        authStatus,
        refreshAuth,
        theme,
        effectiveTheme,
        setTheme,
        hiddenFollowPlatforms,
        toggleFollowVisible,
        disabledPlatforms,
        togglePlatformEnabled,
      }}
    >
      <div className="flex h-screen w-screen overflow-hidden">
        <SideNav />
        <div className="flex min-w-0 flex-1 flex-col">
          <TopBar />
          <main className="min-h-0 flex-1 overflow-y-auto p-6">
            {page === 'home' && <HomePage />}
            {page === 'watch' && <WatchPage />}
            {page === 'favorites' && <FavoritesPage />}
            {page === 'history' && <HistoryPage />}
            {page === 'settings' && <SettingsPage />}
          </main>
        </div>
      </div>
    </AppContext.Provider>
  );
}
