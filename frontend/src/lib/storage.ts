// 关注与观看历史的本地存储（阶段⑤ 将切换到 SQLite 后端）
import { useCallback, useEffect, useState } from 'react';
import type { Room } from './types';

const FAV_KEY = 'streamflow.favorites';
const HIS_KEY = 'streamflow.history';

export interface FavoriteItem extends Room {
  addedAt: number;
}

export interface HistoryItem extends Room {
  openedAt: number;
}

function load<T>(key: string): T[] {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T[]) : [];
  } catch {
    return [];
  }
}

function save<T>(key: string, items: T[]): void {
  try {
    localStorage.setItem(key, JSON.stringify(items));
  } catch {
    // 存储满/不可用时静默降级
  }
}

/** 收藏列表：唯一键 = platform + id */
export function useFavorites() {
  const [favorites, setFavorites] = useState<FavoriteItem[]>(() => load(FAV_KEY));

  useEffect(() => save(FAV_KEY, favorites), [favorites]);

  const isFavorite = useCallback(
    (room: Room) => favorites.some((f) => f.platform === room.platform && f.id === room.id),
    [favorites]
  );

  const toggleFavorite = useCallback((room: Room) => {
    setFavorites((prev) => {
      const exists = prev.some((f) => f.platform === room.platform && f.id === room.id);
      if (exists) {
        return prev.filter((f) => !(f.platform === room.platform && f.id === room.id));
      }
      return [{ ...room, addedAt: Date.now() }, ...prev];
    });
  }, []);

  return { favorites, isFavorite, toggleFavorite };
}

/** 观看历史：最新在前，默认保留 200 条 */
export function useHistory() {
  const [history, setHistory] = useState<HistoryItem[]>(() => load(HIS_KEY));

  useEffect(() => save(HIS_KEY, history), [history]);

  const addHistory = useCallback((room: Room) => {
    setHistory((prev) => {
      const rest = prev.filter((h) => !(h.platform === room.platform && h.id === room.id));
      return [{ ...room, openedAt: Date.now() }, ...rest].slice(0, 200);
    });
  }, []);

  const clearHistory = useCallback(() => setHistory([]), []);

  return { history, addHistory, clearHistory };
}
