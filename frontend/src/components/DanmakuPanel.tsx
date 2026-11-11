// 弹幕面板：只读展示平台弹幕，自动滚动到底部
import { useEffect, useRef, useState } from 'react';
import {
  onDanmakuMessage,
  onDanmakuStatus,
  startDanmaku,
  stopDanmaku,
  type DanmakuConnectionStatus,
  type DanmakuMessage,
} from '../lib/tauri';

const STATUS_TEXT: Record<DanmakuConnectionStatus, string> = {
  connecting: '连接中',
  connected: '已连接',
  error: '连接失败',
  closed: '已断开',
};

const STATUS_DOT: Record<DanmakuConnectionStatus, string> = {
  connecting: 'bg-warning animate-pulse',
  connected: 'bg-success',
  error: 'bg-danger',
  closed: 'bg-slate',
};

/** 已接入只读弹幕的平台 */
const DANMAKU_PLATFORMS = new Set(['bilibili', 'douyu', 'huya', 'douyin', 'kuaishou']);

export function DanmakuPanel({ platform, roomId }: { platform: string; roomId: string }) {
  const supported = DANMAKU_PLATFORMS.has(platform);
  const [messages, setMessages] = useState<DanmakuMessage[]>([]);
  const [status, setStatus] = useState<DanmakuConnectionStatus>(supported ? 'connecting' : 'closed');
  const [error, setError] = useState<string | null>(null);
  const [nonce, setNonce] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);
  const stickToBottom = useRef(true);

  useEffect(() => {
    setMessages([]);
    setStatus(supported ? 'connecting' : 'closed');
    setError(null);
    stickToBottom.current = true;
    if (!supported) return;

    let disposed = false;
    const unlisteners: Array<() => void> = [];

    (async () => {
      unlisteners.push(
        await onDanmakuMessage((msg) => {
          if (disposed || msg.platform !== platform || msg.roomId !== roomId) return;
          setMessages((prev) => [...prev.slice(-199), msg]);
        }),
      );
      unlisteners.push(
        await onDanmakuStatus((event) => {
          if (disposed || event.platform !== platform || event.roomId !== roomId) return;
          setStatus(event.status);
          setError(event.status === 'error' ? (event.message ?? '弹幕连接失败') : null);
        }),
      );
      try {
        await startDanmaku(platform, roomId);
      } catch (e) {
        if (!disposed) {
          setStatus('error');
          setError(String(e));
        }
      }
    })();

    return () => {
      disposed = true;
      unlisteners.forEach((fn) => fn());
      void stopDanmaku(platform, roomId).catch(() => undefined);
    };
  }, [platform, roomId, nonce, supported]);

  // 接近底部时自动跟随新弹幕
  useEffect(() => {
    const el = listRef.current;
    if (el && stickToBottom.current) el.scrollTop = el.scrollHeight;
  }, [messages]);

  const handleScroll = () => {
    const el = listRef.current;
    if (!el) return;
    stickToBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 48;
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex items-center gap-2 px-4 py-2 text-[11px] text-slate">
        <span className={`h-1.5 w-1.5 rounded-full ${STATUS_DOT[status]}`} />
        <span>{STATUS_TEXT[status]}</span>
        <span className="ml-auto tabular-nums">{messages.length} 条</span>
        {(status === 'error' || status === 'closed') && supported && (
          <button
            onClick={() => setNonce((n) => n + 1)}
            className="rounded border border-line px-2 py-0.5 text-[10px] text-slate transition-colors hover:border-stream hover:text-stream"
          >
            重连
          </button>
        )}
      </div>

      <div
        ref={listRef}
        onScroll={handleScroll}
        className="min-h-0 flex-1 space-y-2 overflow-y-auto px-4 pb-4"
      >
        {messages.length === 0 ? (
          <div className="grid h-full place-items-center px-2 text-center text-xs text-slate">
            {!supported
              ? '该平台暂未接入弹幕'
              : status === 'error'
                ? error
                : status === 'connected'
                  ? '暂无弹幕，稍等…'
                  : '正在连接弹幕…'}
          </div>
        ) : (
          messages.map((msg, i) => (
            <div key={`${msg.timestamp}-${i}`} className="text-xs leading-relaxed">
              {msg.color && msg.color.toUpperCase() !== '#FFFFFF' && (
                <span
                  className="mr-1 inline-block h-2 w-2 rounded-full align-middle"
                  style={{ backgroundColor: msg.color }}
                />
              )}
              <span className="text-slate">{msg.username || '匿名'}：</span>
              <span className="text-ink">{msg.text}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
