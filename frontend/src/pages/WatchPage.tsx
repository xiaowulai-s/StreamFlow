// 直播观看：左侧主播放器 + 右侧信息栏 + 底部操作条（产品方案 5.5）
// 播放能力路由：m3u8 → hls.js，flv → mpegts.js；获取失败降级外部打开
import { useEffect, useRef, useState, type ReactNode } from 'react';
import Hls from 'hls.js';
import mpegts from 'mpegts.js';
import { useApp } from '../context';
import { getStreamUrls } from '../lib/tauri';
import { PlatformBadge } from '../components/PlatformBadge';
import { EmptyState } from '../components/EmptyState';
import {
  AlertIcon,
  MuteIcon,
  VolumeIcon,
  MaximizeIcon,
  RefreshIcon,
  ExternalLinkIcon,
  StarIcon,
  StarFilledIcon,
} from '../components/icons';

type PlayerHandle = { destroy: () => void };

export function WatchPage() {
  const { watching, navigate, isFavorite, toggleFavorite } = useApp();
  const videoRef = useRef<HTMLVideoElement>(null);
  const [streamUrl, setStreamUrl] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [muted, setMuted] = useState(false);
  const [reloadKey, setReloadKey] = useState(0);
  const [copied, setCopied] = useState(false);

  // 房间变化 / 手动刷新 → 获取直播流地址
  useEffect(() => {
    if (!watching) return;
    setLoading(true);
    setLoadError(null);
    setStreamUrl(null);
    getStreamUrls(watching.platform, watching.id)
      .then((r) => {
        if (r.url && r.url.length > 0) {
          setStreamUrl(r.url[0]);
        } else {
          setLoadError('未获取到直播流地址');
        }
      })
      .catch((err) => {
        const msg = String(err);
        const m = msg.match(/https?:\/\/[^"\s}]+/);
        setLoadError(
          m ? `该平台暂不支持内嵌播放，可复制链接在浏览器打开：${m[0]}` : `获取直播流失败：${msg}`
        );
      })
      .finally(() => setLoading(false));
  }, [watching, reloadKey]);

  // 播放器创建与销毁（m3u8 走 hls.js，flv 走 mpegts.js）
  useEffect(() => {
    const video = videoRef.current;
    if (!streamUrl || !video) return;

    let handle: PlayerHandle | null = null;

    if (streamUrl.includes('.flv')) {
      if (mpegts.getFeatureList().mseLivePlayback) {
        const player = mpegts.createPlayer(
          { type: 'flv', isLive: true, url: streamUrl },
          { enableStashBuffer: false }
        );
        player.attachMediaElement(video);
        player.load();
        handle = player;
      } else {
        setLoadError('当前环境不支持 flv 播放（MSE 不可用）');
        return;
      }
    } else if (Hls.isSupported()) {
      const hls = new Hls();
      hls.loadSource(streamUrl);
      hls.attachMedia(video);
      handle = hls;
    } else if (video.canPlayType('application/vnd.apple.mpegurl')) {
      video.src = streamUrl;
    } else {
      setLoadError('当前环境不支持 m3u8 播放');
      return;
    }

    return () => {
      try {
        handle?.destroy();
      } catch {
        // 忽略销毁异常
      }
    };
  }, [streamUrl]);

  if (!watching) {
    return (
      <EmptyState
        title="未在观看任何直播间"
        description="从首页/发现/关注列表选择一个直播间进入观看"
        action={{ label: '去发现', onClick: () => navigate('discover') }}
      />
    );
  }

  const externalUrl =
    loadError?.match(/https?:\/\/[^"\s}]+/)?.[0] ??
    (watching.platform === 'bilibili'
      ? `https://live.bilibili.com/${watching.id}`
      : `https://www.${watching.platform}.com/${watching.id}`);

  const copyExternal = async () => {
    try {
      await navigator.clipboard.writeText(externalUrl);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // 剪贴板不可用时静默
    }
  };

  const toggleMute = () => {
    const video = videoRef.current;
    if (!video) return;
    video.muted = !video.muted;
    setMuted(video.muted);
  };

  const goFullscreen = () => videoRef.current?.requestFullscreen?.().catch(() => undefined);

  return (
    <div className="flex h-full gap-4">
      {/* 左侧主播放器 */}
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="relative flex aspect-video w-full items-center justify-center rounded-card bg-ink">
          <video ref={videoRef} controls autoPlay className="h-full w-full rounded-card object-contain" />
          {loading && (
            <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 rounded-card bg-ink/80 text-sm text-white">
              <span className="h-6 w-6 animate-spin rounded-full border-2 border-white/30 border-t-white" />
              正在连接直播间…
            </div>
          )}
          {!loading && loadError && (
            <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 rounded-card bg-ink/90 px-8 text-center">
              <span className="text-danger">
                <AlertIcon size={34} />
              </span>
              <p className="text-sm font-medium text-white">
                {watching.status !== 'live'
                  ? '该主播当前未开播（休眠中），开播后即可观看'
                  : loadError}
              </p>
              {watching.status === 'live' && <p className="text-xs text-white/70">{loadError}</p>}
              <div className="flex gap-2">
                <button
                  onClick={() => setReloadKey((k) => k + 1)}
                  className="flex items-center gap-1.5 rounded-lg bg-stream px-4 py-1.5 text-xs font-medium text-white hover:opacity-90"
                >
                  <RefreshIcon size={14} />
                  重试
                </button>
                <button
                  onClick={copyExternal}
                  className="flex items-center gap-1.5 rounded-lg border border-white/30 px-4 py-1.5 text-xs text-white hover:bg-white/10"
                >
                  <ExternalLinkIcon size={14} />
                  {copied ? '已复制 ✓' : '复制外部链接'}
                </button>
              </div>
            </div>
          )}
        </div>

        {/* 底部操作条 */}
        <div className="mt-3 flex items-center gap-2 rounded-card border border-line bg-surface px-3 py-2">
          <OperationButton
            onClick={toggleMute}
            icon={muted ? <MuteIcon size={15} /> : <VolumeIcon size={15} />}
            label={muted ? '已静音' : '静音'}
          />
          <OperationButton onClick={goFullscreen} icon={<MaximizeIcon size={15} />} label="全屏" />
          <OperationButton onClick={() => setReloadKey((k) => k + 1)} icon={<RefreshIcon size={15} />} label="刷新播放" />
          <span className="flex-1" />
          <OperationButton
            onClick={copyExternal}
            icon={<ExternalLinkIcon size={15} />}
            label={copied ? '已复制 ✓' : '外部打开'}
          />
        </div>
      </div>

      {/* 右侧信息栏 */}
      <aside className="flex w-80 shrink-0 flex-col rounded-card border border-line bg-surface p-5">
        <div className="flex items-center gap-2">
          <PlatformBadge platform={watching.platform} />
          <span className="text-xs text-slate">房间 {watching.id}</span>
        </div>
        <h2 className="mt-3 line-clamp-2 text-lg font-bold leading-snug text-ink">{watching.title}</h2>
        <p className="mt-2 text-sm text-slate">{watching.username || '未知主播'}</p>

        <button
          onClick={() => toggleFavorite(watching)}
          className={`mt-5 flex items-center justify-center gap-2 rounded-lg px-4 py-2 text-sm font-medium transition-colors ${
            isFavorite(watching)
              ? 'border border-line bg-canvas text-slate hover:text-danger'
              : 'brand-gradient text-white hover:opacity-90'
          }`}
        >
          {isFavorite(watching) ? <StarFilledIcon size={16} /> : <StarIcon size={16} />}
          {isFavorite(watching) ? '已收藏（点击取消）' : '收藏主播'}
        </button>

        <div className="mt-6 space-y-2 border-t border-line pt-4 text-xs text-slate">
          <div className="flex justify-between">
            <span>观看人数</span>
            <span className="text-ink">
              {watching.viewCount > 0 ? watching.viewCount.toLocaleString() : '—（接口未提供）'}
            </span>
          </div>
          <div className="flex justify-between">
            <span>弹幕</span>
            <span>暂未接入</span>
          </div>
          <div className="flex justify-between">
            <span>站内播放</span>
            <span className="text-success">已验证支持</span>
          </div>
        </div>
      </aside>
    </div>
  );
}

function OperationButton({
  onClick,
  label,
  icon,
}: {
  onClick: () => void;
  label: string;
  icon: ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className="flex items-center gap-1.5 rounded-lg border border-line px-3 py-1.5 text-xs font-medium text-slate transition-colors hover:border-stream hover:text-stream"
    >
      {icon}
      {label}
    </button>
  );
}
