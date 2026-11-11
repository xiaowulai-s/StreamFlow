// 应用全局上下文：页面路由、观看动作、收藏/历史（阶段⑤ 切 SQLite）
import { createContext, useContext } from 'react';
import type { Room } from './lib/types';
import type { FavoriteItem, HistoryItem } from './lib/storage';
import type { AuthStatus } from './lib/tauri';
import type { EffectiveTheme, ThemePref } from './lib/theme';

export type Page = 'home' | 'watch' | 'favorites' | 'history' | 'settings';

export interface AppContextType {
  page: Page;
  navigate: (p: Page) => void;
  /** 当前正在观看的房间（观看页数据源） */
  watching: Room | null;
  /** 从任意卡片进入观看页：切换页面 + 记录历史 */
  watch: (room: Room) => void;
  /** 顶栏全局搜索关键词（跨页共享） */
  globalKeyword: string;
  setGlobalKeyword: (kw: string) => void;
  favorites: FavoriteItem[];
  isFavorite: (room: Room) => boolean;
  toggleFavorite: (room: Room) => void;
  /** 批量合并平台导入的关注，返回新增数量 */
  mergeFavorites: (rooms: Room[]) => number;
  history: HistoryItem[];
  clearHistory: () => void;
  /** 三平台登录状态（全局共享，设置页操作后刷新） */
  authStatus: AuthStatus[] | null;
  refreshAuth: () => void;
  /** 主题偏好（system/light/dark）与当前实际主题 */
  theme: ThemePref;
  effectiveTheme: EffectiveTheme;
  setTheme: (theme: ThemePref) => void;
  /** 不展示关注的平台集合（持久化到 app_settings） */
  hiddenFollowPlatforms: Set<string>;
  /** 切换某平台关注是否展示 */
  toggleFollowVisible: (platform: string) => void;
  /** 已停用的平台集合（搜索/卡片入口不显示，持久化到 app_settings） */
  disabledPlatforms: Set<string>;
  /** 切换某平台启用/停用 */
  togglePlatformEnabled: (platform: string) => void;
}

export const AppContext = createContext<AppContextType | null>(null);

export function useApp(): AppContextType {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error('useApp 必须在 AppContext.Provider 内使用');
  return ctx;
}
