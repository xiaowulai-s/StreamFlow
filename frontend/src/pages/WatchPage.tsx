// 直播观看：左侧主播放器 + 右侧信息栏 + 底部操作条（产品方案 5.5）
// 播放能力路由：m3u8 → hls.js，flv → mpegts.js；获取失败降级外部打开
import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import Hls from 'hls.js';
import mpegts from 'mpegts.js';
import { useApp } from '../context';
import { getRoomDetail, getStreamUrls, getSetting, openExternal, setSetting } from '../lib/tauri';
import { platformRoomUrl, type QualityOption, type Room } from '../lib/types';
import { PlatformBadge } from '../components/PlatformBadge';
import { EmptyState } from '../components/EmptyState';
import { DanmakuPanel } from '../components/DanmakuPanel';
import {
  DanmakuOverlay,
  OverlaySettings,
  loadOverlayConfig,
  DEFAULT_CONFIG,
  type OverlayConfig,
} from '../components/DanmakuOverlay';
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

// platformRoomUrl 已提升到 lib/types（HomePage/WatchPage 共用）

export function WatchPage() {
  const { watching, navigate, isFavorite, toggleFavorite } = useApp();
  const videoRef = useRef<HTMLVideoElement>(null);
  const playerBoxRef = useRef<HTMLDivElement>(null);
  const [streamUrl, setStreamUrl] = useState<string | null>(null);
  const [qualities, setQualities] = useState<QualityOption[]>([]);
  const [activeQuality, setActiveQuality] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [muted, setMuted] = useState(false);
  const [reloadKey, setReloadKey] = useState(0);
  const [copied, setCopied] = useState(false);
  const [externalOpened, setExternalOpened] = useState(false);
  const [tab, setTab] = useState<'danmaku' | 'info'>('danmaku');
  // 飘屏（横向滚动弹幕）配置：加载一次，弹幕区工具条可调
  const [overlayCfg, setOverlayCfg] = useState<OverlayConfig>(DEFAULT_CONFIG);
  const [overlayPanel, setOverlayPanel] = useState(false);
  useEffect(() => {
    void loadOverlayConfig().then(setOverlayCfg).catch(() => undefined);
  }, []);
  const handleOverlayCfg = useCallback((cfg: OverlayConfig) => {
    setOverlayCfg(cfg);
  }, []);

  // 播放设置（6.2）：音量记忆 + 默认静音，进房时恢复
  useEffect(() => {
    (async () => {
      try {
        const [vol, dm] = await Promise.all([getSetting('player_volume'), getSetting('player_default_muted')]);
        const video = videoRef.current;
        if (video && vol) {
          const v = Math.min(1, Math.max(0, Number(vol) || 0));
          video.volume = v;
        }
        if (dm === 'true') {
          setMuted(true);
          if (video) video.muted = true;
        }
      } catch {
        // 设置不可用时用默认值
      }
    })();
  }, [watching?.id]);

  // 房间详情：覆盖进入时的卡片信息（真实标题/观看人数/开播状态）
  const [detail, setDetail] = useState<Room | null>(null);
  useEffect(() => {
    if (!watching) return;
    let disposed = false;
    setDetail(null);
    getRoomDetail(watching.platform, watching.id)
      .then((d) => {
        if (!disposed) setDetail(d);
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
    };
  }, [watching?.id]);
  // 房间变化 / 手动刷新 → 获取直播流地址
  useEffect(() => {
    if (!watching) return;
    setLoading(true);
    setLoadError(null);
    setStreamUrl(null);
    setQualities([]);
    setActiveQuality(null);
    getStreamUrls(watching.platform, watching.id)
      .then((r) => {
        if (r.url && r.url.length > 0) {
          setStreamUrl(r.url[0]);
          // 画质列表：后端已排序（默认/最高优先）；只保留与首 URL 不同的档位
          const qs = (r.quality ?? []).filter((q) => q.url);
          setQualities(qs.length > 1 ? qs : []);
          setActiveQuality(qs.find((q) => q.url === r.url[0])?.label ?? null);
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

  // 画质切换：换源重建播放器（后端已保证首项为默认/免费最高）
  const switchQuality = (q: QualityOption) => {
    if (q.url === streamUrl) return;
    setActiveQuality(q.label);
    setStreamUrl(q.url);
  };

  // 音量变化持久化（6.2 音量记忆）：静音状态不在此覆盖（由设置页"默认静音"管理）
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;
    let lastSaved = '';
    const persist = () => {
      const v = video.volume.toFixed(2);
      if (v === lastSaved) return;
      lastSaved = v;
      void setSetting('player_volume', v).catch(() => undefined);
    };
    video.addEventListener('volumechange', persist);
    return () => video.removeEventListener('volumechange', persist);
  }, [streamUrl]);

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
        description="从首页搜索、热门直播或关注列表选择一个直播间进入观看"
        action={{ label: '回首页', onClick: () => navigate('home') }}
      />
    );
  }

  // 展示信息：详情优先（人数/真实标题等搜索卡片上没有的字段）
  const info = detail ?? watching;

  const externalUrl =
    loadError?.match(/https?:\/\/[^"\s}]+/)?.[0] ??
    platformRoomUrl(watching.platform, watching.id);

  const copyExternal = async () => {
    try {
      await navigator.clipboard.writeText(externalUrl);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // 剪贴板不可用时静默
    }
  };

  // 用系统默认浏览器打开房间页（6.5 系统浏览器外部打开）
  const handleOpenExternal = async () => {
    try {
      await openExternal(externalUrl);
      setExternalOpened(true);
      setTimeout(() => setExternalOpened(false), 2000);
      // 注：不自动写入按平台记忆（曾导致所有卡片点击跳浏览器）；按平台偏好如需配置走设置页
    } catch {
      // 打开失败时静默（外部链接已可复制）
    }
  };

  const toggleMute = () => {
    const video = videoRef.current;
    if (!video) return;
    video.muted = !video.muted;
    setMuted(video.muted);
  };

  // 全屏用播放器容器（video+飘屏一起进全屏；原生 video 全屏不含兄弟 overlay）
  const goFullscreen = () => playerBoxRef.current?.requestFullscreen?.().catch(() => undefined);

  return (
    <div className="flex h-full gap-4">
      {/* 左侧主播放器 */}
      <div className="flex min-w-0 flex-1 flex-col">
        <div ref={playerBoxRef} className="relative flex aspect-video w-full items-center justify-center rounded-card bg-stage ring-1 ring-black/20">
          <video ref={videoRef} controls autoPlay controlsList="nofullscreen" className="h-full w-full rounded-card object-contain" />
          {/* 飘屏：视频画面顶部横向滚动弹幕层 */}
          <DanmakuOverlay platform={watching.platform} roomId={watching.id} config={overlayCfg} />
          {loading && (
            <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 rounded-card bg-stage/80 text-sm text-white">
              <span className="h-6 w-6 animate-spin rounded-full border-2 border-white/30 border-t-white" />
              正在连接直播间…
            </div>
          )}
          {!loading && loadError && (
            <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 rounded-card bg-stage/90 px-8 text-center">
              <span className="text-danger">
                <AlertIcon size={34} />
              </span>
              <p className="text-sm font-medium text-white">
                {info.status !== 'live'
                  ? '该主播当前未开播（休眠中），开播后即可观看'
                  : loadError}
              </p>
              {info.status === 'live' && <p className="text-xs text-white/70">{loadError}</p>}
              <div className="flex gap-2">
                <button
                  onClick={() => setReloadKey((k) => k + 1)}
                  className="flex items-center gap-1.5 rounded-lg bg-stream px-4 py-1.5 text-xs font-medium text-white hover:opacity-90 dark:text-stage"
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
          {!loading && !loadError && qualities.length > 1 && (
            <div className="absolute right-3 top-3 flex items-center gap-1 rounded-lg bg-black/60 px-1.5 py-1 backdrop-blur">
              {qualities.map((q) => (
                <button
                  key={q.label + q.url}
                  onClick={() => switchQuality(q)}
                  className={`rounded px-2 py-1 text-[11px] transition-colors ${
                    q.label === activeQuality
                      ? 'bg-stream text-white'
                      : 'text-white/80 hover:bg-white/10 hover:text-white'
                  }`}
                  title={q.label === activeQuality ? '当前画质（后端已默认选择免费最高档）' : `切换到 ${q.label}`}
                >
                  {q.label}
                </button>
              ))}
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
            onClick={handleOpenExternal}
            icon={<ExternalLinkIcon size={15} />}
            label={externalOpened ? '已在浏览器打开 ✓' : '外部打开'}
          />
          <OperationButton
            onClick={copyExternal}
            icon={<ExternalLinkIcon size={15} />}
            label={copied ? '已复制 ✓' : '复制链接'}
          />
        </div>
      </div>

      {/* 右侧信息 / 弹幕栏 */}
      <aside className="flex w-80 shrink-0 flex-col overflow-hidden rounded-card border border-line bg-surface">
        <div className="shrink-0 border-b border-line p-5 pb-4">
          <div className="flex items-center gap-2">
            <PlatformBadge platform={watching.platform} />
            <span className="text-xs text-slate">房间 {watching.id}</span>
          </div>
          <h2 className="mt-3 line-clamp-2 text-lg font-bold leading-snug text-ink">
            {info.title}
          </h2>
          <p className="mt-2 text-sm text-slate">{info.username || '未知主播'}</p>

          <button
            onClick={() => toggleFavorite(watching)}
            className={`mt-4 flex w-full items-center justify-center gap-2 rounded-lg px-4 py-2 text-sm font-medium transition-colors ${
              isFavorite(watching)
                ? 'border border-line bg-canvas text-slate hover:text-danger'
                : 'brand-gradient text-white hover:opacity-90'
            }`}
          >
            {isFavorite(watching) ? <StarFilledIcon size={16} /> : <StarIcon size={16} />}
            {isFavorite(watching) ? '已收藏（点击取消）' : '收藏主播'}
          </button>
        </div>

        {/* 标签：弹幕 / 房间信息 */}
        <div className="flex shrink-0 items-center gap-1 border-b border-line px-3">
          {(['danmaku', 'info'] as const).map((key) => (
            <button
              key={key}
              onClick={() => setTab(key)}
              className={`border-b-2 px-3 py-2 text-xs font-medium transition-colors ${
                tab === key
                  ? 'border-stream text-stream'
                  : 'border-transparent text-slate hover:text-ink'
              }`}
            >
              {key === 'danmaku' ? '弹幕' : '房间信息'}
            </button>
          ))}
          {tab === 'danmaku' && (
            <button
              onClick={() => setOverlayPanel((v) => !v)}
              title="飘屏设置"
              className={`ml-auto rounded px-2 py-1 text-xs ring-1 transition-colors ${
                overlayCfg.enabled
                  ? 'text-stream ring-stream/40 hover:bg-black/5'
                  : 'text-slate ring-line hover:text-ink'
              }`}
            >
              飘屏{overlayCfg.enabled ? '·开' : '·关'}
            </button>
          )}
        </div>

        {/* 飘屏设置：内联展开（不遮挡下方列表） */}
        {tab === 'danmaku' && overlayPanel && (
          <div className="shrink-0 border-b border-line px-3 py-2">
            <OverlaySettings config={overlayCfg} onChange={handleOverlayCfg} />
          </div>
        )}

        <div className="min-h-0 flex-1">
          {tab === 'danmaku' ? (
            <DanmakuPanel platform={watching.platform} roomId={watching.id} />
          ) : (
            <div className="space-y-2 p-5 text-xs text-slate">
              <div className="flex justify-between">
                <span>观看人数</span>
                <span className="text-ink">
                  {info.viewCount > 0 ? info.viewCount.toLocaleString() : '—（接口未提供）'}
                </span>
              </div>
              <div className="flex justify-between">
                <span>弹幕</span>
                <span className="text-success">只读显示</span>
              </div>
              <div className="flex justify-between">
                <span>站内播放</span>
                <span className="text-success">已验证支持</span>
              </div>
              <p className="pt-2 text-[10px] leading-relaxed">
                弹幕通过平台公开接口只读接入，不发送、不注入平台。
              </p>
            </div>
          )}
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
