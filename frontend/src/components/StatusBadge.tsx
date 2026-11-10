// 直播状态徽章：状态可信原则（产品方案 3.3）——直播中/休眠/错误可区分
export type Status = 'live' | 'sleeping' | 'error';

const CONFIG: Record<Status, { label: string; dot: string; text: string; bg: string }> = {
  live: { label: '直播中', dot: 'bg-success', text: 'text-success', bg: 'bg-emerald-50' },
  sleeping: { label: '休眠', dot: 'bg-slate', text: 'text-slate', bg: 'bg-gray-100' },
  error: { label: '获取失败', dot: 'bg-danger', text: 'text-danger', bg: 'bg-red-50' },
};

export function StatusBadge({ status, size = 'md' }: { status: Status; size?: 'sm' | 'md' }) {
  const c = CONFIG[status] ?? CONFIG.sleeping;
  const pad = size === 'sm' ? 'px-1.5 py-0.5 text-[10px]' : 'px-2 py-0.5 text-xs';
  return (
    <span className={`inline-flex items-center gap-1.5 rounded-full font-medium ${pad} ${c.bg} ${c.text}`}>
      <span className={`h-1.5 w-1.5 rounded-full ${c.dot} ${status === 'live' ? 'animate-pulse' : ''}`} />
      {c.label}
    </span>
  );
}
