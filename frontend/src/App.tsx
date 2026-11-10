// 应用布局壳：左侧导航 + 顶部工具栏 + 页面路由（产品方案 4 信息架构）
import { useState } from 'react';
import { AppContext, type Page } from './context';
import { useFavorites, useHistory } from './lib/storage';
import type { Room } from './lib/types';
import { SideNav } from './components/SideNav';
import { TopBar } from './components/TopBar';
import { HomePage } from './pages/HomePage';
import { DiscoverPage } from './pages/DiscoverPage';
import { WatchPage } from './pages/WatchPage';
import { FavoritesPage } from './pages/FavoritesPage';
import { HistoryPage } from './pages/HistoryPage';

export default function App() {
  const [page, setPage] = useState<Page>('home');
  const [watchRoom, setWatchRoom] = useState<Room | null>(null);
  const [globalKeyword, setGlobalKeyword] = useState('');

  const { favorites, isFavorite, toggleFavorite } = useFavorites();
  const { history, addHistory, clearHistory } = useHistory();

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
        history,
        clearHistory,
      }}
    >
      <div className="flex h-screen w-screen overflow-hidden">
        <SideNav />
        <div className="flex min-w-0 flex-1 flex-col">
          <TopBar />
          <main className="min-h-0 flex-1 overflow-y-auto p-6">
            {page === 'home' && <HomePage />}
            {page === 'discover' && <DiscoverPage />}
            {page === 'watch' && <WatchPage />}
            {page === 'favorites' && <FavoritesPage />}
            {page === 'history' && <HistoryPage />}
          </main>
        </div>
      </div>
    </AppContext.Provider>
  );
}
