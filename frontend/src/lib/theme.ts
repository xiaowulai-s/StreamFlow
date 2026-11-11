// 主题偏好：system / light / dark
// 优先级：SQLite 设置 > localStorage 缓存（首屏防闪烁）> 系统
import { useCallback, useEffect, useState } from 'react';
import { getSetting, setSetting } from './tauri';

export type ThemePref = 'system' | 'light' | 'dark';
export type EffectiveTheme = 'light' | 'dark';

const CACHE_KEY = 'streamflow.theme';
const SETTING_KEY = 'theme';

function isThemePref(v: unknown): v is ThemePref {
  return v === 'system' || v === 'light' || v === 'dark';
}

export function systemPrefersDark(): boolean {
  return typeof window !== 'undefined'
    ? window.matchMedia?.('(prefers-color-scheme: dark)').matches ?? false
    : false;
}

/** 应用主题到 <html>（Tailwind dark 变体 + 原生控件配色） */
export function applyTheme(theme: EffectiveTheme): void {
  const root = document.documentElement;
  root.classList.toggle('dark', theme === 'dark');
  root.style.colorScheme = theme;
}

/** 首屏尽早应用缓存主题，避免亮暗闪烁 */
export function applyCachedTheme(): void {
  let pref: ThemePref = 'system';
  try {
    const raw = localStorage.getItem(CACHE_KEY);
    if (isThemePref(raw)) pref = raw;
  } catch {
    // localStorage 不可用时按系统
  }
  applyTheme(pref === 'system' ? (systemPrefersDark() ? 'dark' : 'light') : pref);
}

export function readCachedTheme(): ThemePref {
  try {
    const raw = localStorage.getItem(CACHE_KEY);
    if (isThemePref(raw)) return raw;
  } catch {
    // ignore
  }
  return 'system';
}

export function useTheme() {
  const [pref, setPrefState] = useState<ThemePref>(readCachedTheme);
  const [systemDark, setSystemDark] = useState<boolean>(systemPrefersDark);

  // 从 SQLite 读取用户选择（覆盖首屏缓存）
  useEffect(() => {
    let cancelled = false;
    getSetting(SETTING_KEY)
      .then((value) => {
        if (!cancelled && isThemePref(value)) setPrefState(value);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);

  // 跟随系统时实时响应系统主题变化
  useEffect(() => {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const onChange = (e: MediaQueryListEvent) => setSystemDark(e.matches);
    mq.addEventListener?.('change', onChange);
    return () => mq.removeEventListener?.('change', onChange);
  }, []);

  const effective: EffectiveTheme =
    pref === 'system' ? (systemDark ? 'dark' : 'light') : pref;

  useEffect(() => {
    applyTheme(effective);
    try {
      localStorage.setItem(CACHE_KEY, pref);
    } catch {
      // ignore
    }
  }, [effective, pref]);

  const setPref = useCallback((next: ThemePref) => {
    setPrefState(next);
    void setSetting(SETTING_KEY, next).catch(() => undefined);
  }, []);

  return { pref, effective, setPref };
}
