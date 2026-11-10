// Tauri invoke 封装（Tauri 2 全局 API 位于 core 命名空间）
import type { Room, StreamResult } from './types';

function rawInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const t = (window as unknown as { __TAURI__?: { core?: { invoke?: unknown } } }).__TAURI__;
  if (!t?.core?.invoke) {
    return Promise.reject(new Error('Tauri API 不可用（请在应用内运行）'));
  }
  return (t.core.invoke as (c: string, a?: Record<string, unknown>) => Promise<T>)(cmd, args);
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
