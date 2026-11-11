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
  /** 本场开播时间（Unix 秒，接口提供时才有） */
  startedAt?: number;
}

export interface StreamResult {
  url: string[];
  quality: QualityOption[];
  platform: string;
  roomId: string;
}

/** 画质选项（后端 quality 列表；快手等多码率平台非空，首项为默认/最高） */
export interface QualityOption {
  label: string;
  url: string;
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
  douyin: true,   // m3u8（webcast reflow）
  kuaishou: true, // flv（分类榜单）
};

/** 各平台直播间外部链接（复制/外部打开用） */
export function platformRoomUrl(platform: Platform, id: string): string {
  switch (platform) {
    case 'bilibili':
      return `https://live.bilibili.com/${id}`;
    case 'douyin':
      return `https://live.douyin.com/${id}`;
    case 'kuaishou':
      return `https://live.kuaishou.com/u/${id}`;
    case 'huya':
      return `https://www.huya.com/${id}`;
    case 'douyu':
      return `https://www.douyu.com/${id}`;
  }
}
