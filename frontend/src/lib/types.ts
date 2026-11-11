// 统一数据类型（与后端 serde 序列化契约对应）

export type Platform = 'bilibili' | 'douyu' | 'huya' | 'douyin' | 'kuaishou';

export interface Room {
  id: string;
  platform: Platform;
  title: string;
  username: string;
  viewCount: number;
  status: 'live' | 'sleeping' | 'error';
  /** 房间封面（接口提供时才有） */
  cover?: string;
}

export interface StreamResult {
  url: string[];
  quality: unknown[];
  platform: string;
  roomId: string;
}

export const PLATFORM_LABELS: Record<Platform, string> = {
  bilibili: 'B站',
  douyu: '斗鱼',
  huya: '虎牙',
  douyin: '抖音',
  kuaishou: '快手',
};

/** 平台在流地址上已验证的能力（与后端 capabilities 对应，见交接文档） */
export const PLATFORM_STREAM_SUPPORT: Record<Platform, boolean> = {
  bilibili: true, // m3u8
  douyu: true,    // flv
  huya: true,     // m3u8
  douyin: false,  // 存根未验证
  kuaishou: false, // 存根未验证
};
