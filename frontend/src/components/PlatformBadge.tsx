// 平台徽标：文字标识（产品方案 3.2 平台可识别，不依赖颜色区分）
import type { Platform } from '../lib/types';
import { PLATFORM_LABELS } from '../lib/types';

const STYLES: Record<Platform, string> = {
  bilibili: 'bg-pink-50 text-pink-600 border-pink-200',
  douyu: 'bg-orange-50 text-orange-600 border-orange-200',
  huya: 'bg-amber-50 text-amber-700 border-amber-200',
  douyin: 'bg-gray-100 text-gray-700 border-gray-300',
  kuaishou: 'bg-cyan-50 text-cyan-700 border-cyan-200',
};

export function PlatformBadge({ platform, size = 'md' }: { platform: Platform; size?: 'sm' | 'md' }) {
  const pad = size === 'sm' ? 'px-1.5 py-0.5 text-[10px]' : 'px-2 py-0.5 text-xs';
  return (
    <span className={`inline-flex items-center rounded border font-medium ${pad} ${STYLES[platform] ?? 'bg-gray-100 text-gray-600 border-gray-200'}`}>
      {PLATFORM_LABELS[platform] ?? platform}
    </span>
  );
}
