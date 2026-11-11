// 关注与观看历史的本地存储（SQLite，首次启动迁移旧 localStorage）
import { useCallback, useEffect, useState } from 'react';
import {
  loadFavorites,
  saveFavorites,
  loadHistory,
  saveHistory,
} from './tauri';
import type { Room } from './types';

const FAV_KEY = 'streamflow.favorites';
const HIS_KEY = 'streamflow.history';

export interface FavoriteItem extends Room {
  addedAt: number;
}

export interface HistoryItem extends Room {
  openedAt: number;
}

/** 读取旧版本 localStorage 数据（仅用于一次性迁移/兜底） */
function loadLegacy<T>(key: string): T[] {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T[]) : [];
  } catch {
    return [];
  }
}

/** SQLite 不可用时的兜底写入 */
function saveLegacy<T>(key: string, items: T[]): void {
  try {
    localStorage.setItem(key, JSON.stringify(items));
  } catch {
    // 存储满/不可用时静默降级
  }
}

/** 收藏列表：唯一键 = platform + id */
export function useFavorites() {
  const [favorites, setFavorites] = useState<FavoriteItem[]>([]);
  const [ready, setReady] = useState(false);

  // 启动加载；数据库为空时迁移旧 localStorage
  useEffect(() => {
    let cancelled = false;
    loadFavorites()
      .then((rows) => {
        if (cancelled) return;
        if (rows.length === 0) {
          const legacy = loadLegacy<FavoriteItem>(FAV_KEY);
          if (legacy.length > 0) {
            setFavorites(legacy);
            setReady(true);
            void saveFavorites(legacy)
              .then(() => localStorage.removeItem(FAV_KEY))
              .catch(() => undefined);
            return;
          }
        }
        setFavorites(rows);
        setReady(true);
      })
      .catch(() => {
        if (cancelled) return;
        setFavorites(loadLegacy<FavoriteItem>(FAV_KEY));
        setReady(true);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // 变更写回 SQLite；失败时兜底 localStorage
  useEffect(() => {
    if (!ready) return;
    saveFavorites(favorites).catch(() => saveLegacy(FAV_KEY, favorites));
  }, [favorites, ready]);

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

  /** 批量合并关注（平台导入用），返回本次新增数量 */
  const mergeFavorites = useCallback(
    (rooms: Room[]): number => {
      const existing = new Set(favorites.map((f) => `${f.platform}:${f.id}`));
      const additions: FavoriteItem[] = [];
      for (const room of rooms) {
        const key = `${room.platform}:${room.id}`;
        if (existing.has(key)) continue;
        existing.add(key);
        additions.push({ ...room, addedAt: Date.now() });
      }
      if (additions.length > 0) {
        setFavorites((prev) => [...additions, ...prev]);
      }
      return additions.length;
    },
    [favorites],
  );

  return { favorites, isFavorite, toggleFavorite, mergeFavorites };
}

/** 观看历史：最新在前，默认保留 200 条 */
export function useHistory() {
  const [history, setHistory] = useState<HistoryItem[]>([]);
  const [ready, setReady] = useState(false);

  useEffect(() => {
    let cancelled = false;
    loadHistory()
      .then((rows) => {
        if (cancelled) return;
        if (rows.length === 0) {
          const legacy = loadLegacy<HistoryItem>(HIS_KEY);
          if (legacy.length > 0) {
            setHistory(legacy);
            setReady(true);
            void saveHistory(legacy)
              .then(() => localStorage.removeItem(HIS_KEY))
              .catch(() => undefined);
            return;
          }
        }
        setHistory(rows);
        setReady(true);
      })
      .catch(() => {
        if (cancelled) return;
        setHistory(loadLegacy<HistoryItem>(HIS_KEY));
        setReady(true);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!ready) return;
    saveHistory(history).catch(() => saveLegacy(HIS_KEY, history));
  }, [history, ready]);

  const addHistory = useCallback((room: Room) => {
    setHistory((prev) => {
      const rest = prev.filter((h) => !(h.platform === room.platform && h.id === room.id));
      return [{ ...room, openedAt: Date.now() }, ...rest].slice(0, 200);
    });
  }, []);

  const clearHistory = useCallback(() => setHistory([]), []);

  return { history, addHistory, clearHistory };
}
