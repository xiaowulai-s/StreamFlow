// Tauri invoke 封装（Tauri 2 全局 API 位于 core 命名空间）
import type { Room, StreamResult } from './types';
import type { FavoriteItem, HistoryItem } from './storage';

function rawInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const t = (window as unknown as { __TAURI__?: { core?: { invoke?: unknown } } }).__TAURI__;
  if (!t?.core?.invoke) {
    return Promise.reject(new Error('Tauri API 不可用（请在应用内运行）'));
  }
  return (t.core.invoke as (c: string, a?: Record<string, unknown>) => Promise<T>)(cmd, args);
}

type UnlistenFn = () => void;

/** 订阅 Tauri 事件（浏览器 dev 环境返回空 unlisten） */
async function listenEvent<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  const t = (
    window as unknown as {
      __TAURI__?: {
        event?: {
          listen?: (
            name: string,
            cb: (e: { payload: T }) => void,
          ) => Promise<UnlistenFn>;
        };
      };
    }
  ).__TAURI__;
  if (!t?.event?.listen) return () => undefined;
  return t.event.listen(event, (e) => handler(e.payload));
}

export function searchRooms(platform: string, keyword: string): Promise<Room[]> {
  return rawInvoke('search_rooms', { platform, keyword });
}

export function getStreamUrls(platform: string, roomId: string): Promise<StreamResult> {
  return rawInvoke('get_stream_urls', { platform, roomId });
}

export function getRoomDetail(platform: string, roomId: string): Promise<Room> {
  return rawInvoke('get_room_detail', { platform, roomId });
}

export function getLiveStatus(platform: string, roomId: string): Promise<Room['status']> {
  return rawInvoke('get_live_status', { platform, roomId });
}

/** 聚合所有已接入平台的热门/推荐直播间 */
export function getHotRooms(): Promise<Room[]> {
  return rawInvoke('get_hot_rooms', {});
}

/** 获取已登录平台的关注/订阅列表 */
export function getPlatformFollows(platform: string): Promise<Room[]> {
  return rawInvoke('get_platform_follows', { platform });
}

// ---------------------------------------------------------------------------
// 平台登录（阶段D：应用内嵌 WebView 登录）
// ---------------------------------------------------------------------------

export interface AuthStatus {
  platform: string;
  label: string;
  loggedIn: boolean;
  username: string | null;
  /** 是否支持应用内登录（抖音 false） */
  canLogin?: boolean;
}

export interface PlatformSession {
  cookie: string;
  username: string | null;
}

/** 打开平台官方登录页窗口（用户在窗口内完成扫码/手机号登录） */
export function openLoginWindow(platform: string): Promise<string> {
  return rawInvoke('open_login_window', { platform });
}

/** 完成登录：读取登录窗口 cookie → 保存 → 注入适配器 → 关闭窗口 */
export function completeLogin(platform: string): Promise<PlatformSession> {
  return rawInvoke('complete_login', { platform });
}

/** 查询三平台登录状态 */
export function getAuthStatus(): Promise<AuthStatus[]> {
  return rawInvoke('get_auth_status', {});
}

/** 退出登录：清除存储与适配器 cookie */
export function logoutPlatform(platform: string): Promise<void> {
  return rawInvoke('logout', { platform });
}

// ---------------------------------------------------------------------------
// 扫码登录（B站 / 斗鱼）
// ---------------------------------------------------------------------------

export interface QrLoginTicket {
  platform: string;
  key: string;
  /** 后端生成的二维码 SVG 标记 */
  qrSvg: string;
}

export type QrLoginStatus = 'pending' | 'scanned' | 'success' | 'expired';

export interface QrLoginPoll {
  status: QrLoginStatus;
  username: string | null;
}

/** 生成平台扫码登录二维码 */
export function startQrLogin(platform: string): Promise<QrLoginTicket> {
  return rawInvoke('start_qr_login', { platform });
}

/** 轮询扫码状态；success 时会话已由后端保存并注入适配器 */
export function pollQrLogin(platform: string, key: string): Promise<QrLoginPoll> {
  return rawInvoke('poll_qr_login', { platform, key });
}

// ---------------------------------------------------------------------------
// 本地 SQLite 存储（收藏 / 历史 / 设置）
// ---------------------------------------------------------------------------

export function loadFavorites(): Promise<FavoriteItem[]> {
  return rawInvoke('load_favorites', {});
}

export function saveFavorites(items: FavoriteItem[]): Promise<void> {
  return rawInvoke('save_favorites', { items });
}

export function loadHistory(): Promise<HistoryItem[]> {
  return rawInvoke('load_history', {});
}

export function saveHistory(items: HistoryItem[]): Promise<void> {
  return rawInvoke('save_history', { items });
}

export function getSetting(key: string): Promise<string | null> {
  return rawInvoke('get_setting', { key });
}

export function setSetting(key: string, value: string): Promise<void> {
  return rawInvoke('set_setting', { key, value });
}

// ---------------------------------------------------------------------------
// 弹幕只读接入
// ---------------------------------------------------------------------------

export interface DanmakuMessage {
  platform: string;
  roomId: string;
  username: string;
  text: string;
  color?: string | null;
  timestamp: number;
}

export type DanmakuConnectionStatus = 'connecting' | 'connected' | 'error' | 'closed';

export interface DanmakuStatusEvent {
  platform: string;
  roomId: string;
  status: DanmakuConnectionStatus;
  message?: string | null;
}

export function startDanmaku(platform: string, roomId: string): Promise<void> {
  return rawInvoke('start_danmaku', { platform, roomId });
}

export function stopDanmaku(platform: string, roomId: string): Promise<void> {
  return rawInvoke('stop_danmaku', { platform, roomId });
}

export function onDanmakuMessage(handler: (msg: DanmakuMessage) => void): Promise<UnlistenFn> {
  return listenEvent('danmaku-message', handler);
}

export function onDanmakuStatus(handler: (status: DanmakuStatusEvent) => void): Promise<UnlistenFn> {
  return listenEvent('danmaku-status', handler);
}

// ---------------------------------------------------------------------------
// 登录态导出 / 导入（跨设备迁移）
// ---------------------------------------------------------------------------

/** 导出登录态到系统「下载」目录，返回导出文件完整路径 */
export function exportAuth(): Promise<string> {
  return rawInvoke('export_auth_file');
}

/** 导入登录态 JSON 文本，返回导入成功的平台列表 */
export function importAuth(payload: string): Promise<string[]> {
  return rawInvoke('import_auth_file', { payload });
}

/** 用系统默认浏览器打开链接 */
export function openExternal(url: string): Promise<void> {
  return rawInvoke('open_external', { url });
}

/** 平台连通性检测，返回延迟毫秒；失败时抛出错误信息 */
export function checkPlatformConnectivity(platform: string): Promise<number> {
  return rawInvoke('check_platform_connectivity', { platform });
}

/** 清理 WebView 网页缓存（会同时清除平台登录 cookie，需重新登录！） */
export function clearWebviewCache(): Promise<void> {
  return rawInvoke('clear_webview_cache');
}

// ---------------------------------------------------------------------------
// 应用内自更新（tauri-plugin-updater）
// ---------------------------------------------------------------------------

export interface UpdateInfo {
  version: string;
  notes: string | null;
  /** 当前版本与最新版本不同时为 true */
  available: boolean;
}

type UpdaterGlobal = {
  __TAURI__?: {
    updater?: {
      check?: () => Promise<
        | null
        | {
            version: string;
            currentVersion: string;
            body?: string | null;
            downloadAndInstall?: (cb?: (e: { event: string; data?: unknown }) => void) => Promise<void>;
          }
      >;
    };
    app?: { getVersion?: () => Promise<string> };
  };
};

/** 读取应用版本（tauri.conf.json version 字段） */
export async function getAppVersion(): Promise<string | null> {
  const t = (window as unknown as UpdaterGlobal).__TAURI__;
  try {
    return (await t?.app?.getVersion?.()) ?? null;
  } catch {
    return null;
  }
}

/** 检查更新：返回最新版本信息；无可更新时返回 null */
export async function checkForUpdate(): Promise<UpdateInfo | null> {
  const t = (window as unknown as UpdaterGlobal).__TAURI__;
  const update = await t?.updater?.check?.();
  if (!update) return null;
  return {
    version: update.version,
    notes: update.body ?? null,
    available: update.version !== update.currentVersion,
  };
}

/** 下载并安装更新（安装完成后应用会自动重启） */
export async function downloadAndInstallUpdate(
  onProgress?: (downloaded: number, total: number | null) => void,
): Promise<void> {
  const t = (window as unknown as UpdaterGlobal).__TAURI__;
  const update = await t?.updater?.check?.();
  if (!update?.downloadAndInstall) throw new Error('没有可安装的更新');
  await update.downloadAndInstall((e) => {
    const data = e.data as { chunkLength?: number; contentLength?: number } | undefined;
    if (e.event === 'Progress' && onProgress && data?.chunkLength) {
      onProgress(data.chunkLength, data?.contentLength ?? null);
    }
  });
}

