// 设置中心：平台接入管理（登录状态/扫码登录/内嵌登录/退出）
// B站、斗鱼走官方扫码接口（后端生成二维码并轮询）；虎牙暂用内嵌登录窗口。
import { useCallback, useEffect, useRef, useState } from 'react';
import {
  checkForUpdate,
  checkPlatformConnectivity,
  clearWebviewCache,
  downloadAndInstallUpdate,
  exportAuth,
  getAppVersion,
  getAuthStatus,
  importAuth,
  openLoginWindow,
  completeLogin,
  logoutPlatform,
  startQrLogin,
  pollQrLogin,
  setSetting,
  getSetting,
  type AuthStatus,
  type QrLoginStatus,
} from '../lib/tauri';
import { PlatformBadge } from '../components/PlatformBadge';
import { EmptyState } from '../components/EmptyState';
import { useApp } from '../context';
import type { Platform } from '../lib/types';

interface QrState {
  platform: string;
  key: string;
  svg: string;
  status: QrLoginStatus;
  error: string | null;
}

/** 支持扫码登录的平台（虎牙 UDB 扫码为私有交换协议，暂走内嵌窗口） */
const QR_PLATFORMS = new Set(['bilibili', 'douyu']);

const QR_STATUS_TEXT: Record<QrLoginStatus, string> = {
  pending: '请使用对应平台 App 扫描二维码',
  scanned: '已扫码，请在手机上确认登录',
  success: '登录成功',
  expired: '二维码已过期，请刷新后重试',
};

export function SettingsPage() {
  const {
    refreshAuth,
    theme,
    effectiveTheme,
    setTheme,
    hiddenFollowPlatforms,
    toggleFollowVisible,
    disabledPlatforms,
    togglePlatformEnabled,
  } = useApp();
  const [status, setStatus] = useState<AuthStatus[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const [qr, setQr] = useState<QrState | null>(null);
  const importInputRef = useRef<HTMLInputElement>(null);
  // 6.2 播放设置
  const [defaultMuted, setDefaultMuted] = useState(false);
  const [defaultOpenMode, setDefaultOpenMode] = useState<'internal' | 'external'>('internal');
  // 6.1 连接检测：platform -> 状态文案
  const [connResults, setConnResults] = useState<Record<string, string>>({});
  // 应用内自更新
  const [appVersion, setAppVersion] = useState<string>('…');
  const [updateState, setUpdateState] = useState<
    | { kind: 'idle' }
    | { kind: 'checking' }
    | { kind: 'latest' }
    | { kind: 'available'; version: string; notes: string | null }
    | { kind: 'downloading' }
    | { kind: 'error'; message: string }
  >({ kind: 'idle' });

  /** 导出登录态到下载目录 */
  const handleExportAuth = async () => {
    try {
      const path = await exportAuth();
      setMessage({ ok: true, text: `登录态已导出：${path}` });
    } catch (e) {
      setMessage({ ok: false, text: String(e) });
    }
  };

  /** 选择导出文件并导入登录态 */
  const handleImportAuth = async (file: File) => {
    setBusy('import');
    try {
      const text = await file.text();
      const platforms = await importAuth(text);
      setMessage({
        ok: true,
        text: `登录态导入成功：${platforms.join('、')}；相关功能即刻生效（导入前数据已自动备份）`,
      });
      refresh();
      refreshAuth();
    } catch (e) {
      setMessage({ ok: false, text: `导入失败：${String(e)}` });
    } finally {
      setBusy(null);
    }
  };

  /** 6.2 播放设置 */
  const handleDefaultMuted = (v: boolean) => {
    setDefaultMuted(v);
    void setSetting('player_default_muted', v ? 'true' : 'false').catch(() => undefined);
  };
  const handleDefaultOpenMode = (v: 'internal' | 'external') => {
    setDefaultOpenMode(v);
    void setSetting('default_open_mode', v).catch(() => undefined);
  };

  /** 6.3 清理网页缓存（会清除平台登录态，需确认） */
  const handleClearCache = async () => {
    if (!window.confirm('清理网页缓存会同时清除所有平台的登录状态（需要重新登录），确定继续吗？')) return;
    setBusy('cache');
    try {
      await clearWebviewCache();
      setMessage({ ok: true, text: '网页缓存已清理；平台登录状态已随之清除，需要重新登录' });
      refresh();
      refreshAuth();
    } catch (e) {
      setMessage({ ok: false, text: `清理失败：${String(e)}` });
    } finally {
      setBusy(null);
    }
  };

  /** 6.1 平台连通性检测 */
  const handleCheckConn = async (platform: string) => {
    setConnResults((prev) => ({ ...prev, [platform]: '检测中…' }));
    try {
      const ms = await checkPlatformConnectivity(platform);
      setConnResults((prev) => ({ ...prev, [platform]: `正常（${ms}ms）` }));
    } catch (e) {
      setConnResults((prev) => ({ ...prev, [platform]: `异常：${String(e).slice(0, 60)}` }));
    }
  };

  const refresh = useCallback(() => {
    getAuthStatus()
      .then(setStatus)
      .catch(() => setStatus([]));
  }, []);

  useEffect(() => {
    refresh();
    // 读取播放设置现状（6.2）
    void getSetting('player_default_muted').then((v) => setDefaultMuted(v === 'true')).catch(() => undefined);
    void getSetting('default_open_mode').then((v) => {
      if (v === 'internal' || v === 'external') setDefaultOpenMode(v);
    }).catch(() => undefined);
    // 应用版本（tauri.conf.json）
    void getAppVersion().then((v) => setAppVersion(v ?? '未知')).catch(() => setAppVersion('未知'));
  }, [refresh]);

  /** 检查并安装更新（GitHub Releases latest.json） */
  const handleCheckUpdate = async () => {
    setUpdateState({ kind: 'checking' });
    try {
      const info = await checkForUpdate();
      if (!info || info.version === info.currentVersion) {
        setUpdateState({ kind: 'latest' });
        return;
      }
      setUpdateState({ kind: 'available', version: info.version, notes: info.notes });
    } catch (e) {
      setUpdateState({ kind: 'error', message: String(e) });
    }
  };

  const handleInstallUpdate = async () => {
    setUpdateState({ kind: 'downloading' });
    try {
      await downloadAndInstallUpdate();
      // 安装完成由后端重启应用，不回到 idle
    } catch (e) {
      setUpdateState({ kind: 'error', message: String(e) });
    }
  };

  const labelOf = useCallback(
    (platform: string) => status?.find((s) => s.platform === platform)?.label ?? platform,
    [status],
  );

  const openQr = async (platform: string) => {
    setBusy(platform);
    try {
      const ticket = await startQrLogin(platform);
      setQr({
        platform: ticket.platform,
        key: ticket.key,
        svg: ticket.qrSvg,
        status: 'pending',
        error: null,
      });
    } catch (e) {
      setMessage({ ok: false, text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  // 轮询扫码状态：pending → scanned → success / expired
  useEffect(() => {
    if (!qr || qr.status === 'success' || qr.status === 'expired') return;
    const { platform, key } = qr;
    let cancelled = false;
    const timer = window.setInterval(async () => {
      try {
        const result = await pollQrLogin(platform, key);
        if (cancelled) return;
        if (result.status === 'success') {
          setQr((prev) =>
            prev && prev.key === key ? { ...prev, status: 'success', error: null } : prev,
          );
          setMessage({
            ok: true,
            text: `登录成功${result.username ? `：${result.username}` : ''}`,
          });
          refresh();
          refreshAuth();
        } else if (result.status === 'expired') {
          setQr((prev) =>
            prev && prev.key === key ? { ...prev, status: 'expired', error: null } : prev,
          );
        } else {
          setQr((prev) =>
            prev && prev.key === key && prev.status !== result.status
              ? { ...prev, status: result.status, error: null }
              : prev,
          );
        }
      } catch (e) {
        if (!cancelled) {
          setQr((prev) => (prev && prev.key === key ? { ...prev, error: String(e) } : prev));
        }
      }
    }, 2000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [qr?.platform, qr?.key, qr?.status, refresh]);

  const handleOpen = async (platform: string) => {
    try {
      await openLoginWindow(platform);
      setMessage({
        ok: true,
        text: '平台窗口已打开——请在窗口内完成登录，完成后回到本页点击「完成登录」',
      });
    } catch (e) {
      setMessage({ ok: false, text: String(e) });
    }
  };

  const handleComplete = async (platform: string) => {
    setBusy(platform);
    try {
      const session = await completeLogin(platform);
      setMessage({
        ok: true,
        text: `登录成功${session.username ? `：${session.username}` : ''}`,
      });
    } catch (e) {
      setMessage({ ok: false, text: String(e) });
    } finally {
      setBusy(null);
      refresh();
      refreshAuth();
    }
  };

  const handleLogout = async (platform: string) => {
    setBusy(platform);
    try {
      await logoutPlatform(platform);
      setMessage({ ok: true, text: '已退出登录' });
    } catch (e) {
      setMessage({ ok: false, text: String(e) });
    } finally {
      setBusy(null);
      refresh();
      refreshAuth();
    }
  };

  if (status === null) {
    return (
      <div className="grid gap-4">
        {Array.from({ length: 3 }).map((_, i) => (
          <div key={i} className="h-24 animate-pulse rounded-card border border-line bg-surface" />
        ))}
      </div>
    );
  }

  if (status.length === 0) {
    return (
      <EmptyState
        title="无法获取平台状态"
        description="Tauri API 不可用（请在应用内运行）"
      />
    );
  }

  return (
    <div className="max-w-2xl space-y-5">
      <div>
        <h2 className="text-base font-bold text-ink">平台接入</h2>
        <p className="mt-1 text-xs text-slate">
          登录后 StreamFlow 将携带你本人的会话访问该平台的关注列表等登录态功能；
          会话仅保存在本机（应用数据目录），可随时退出登录清除。
        </p>
      </div>

      {message && (
        <div
          className={`rounded-lg border px-4 py-3 text-sm ${
            message.ok
              ? 'border-success/30 bg-success/10 text-ink'
              : 'border-danger/30 bg-danger/10 text-ink'
          }`}
        >
          {message.text}
        </div>
      )}

      <div className="space-y-3">
        {status.map((s) => {
          const canQr = QR_PLATFORMS.has(s.platform);
          const canLogin = s.canLogin ?? true;
          return (
            <div
              key={s.platform}
              className={`flex items-center gap-4 rounded-card border border-line bg-surface px-5 py-4 ${
                !canLogin ? 'opacity-75' : ''
              }`}
            >
              <PlatformBadge platform={s.platform as Platform} size="md" />
              <div className="min-w-0 flex-1">
                <div className="text-sm font-medium text-ink">
                  {s.label}
                  {s.loggedIn && (
                    <span className="ml-2 text-xs font-normal text-success">已登录</span>
                  )}
                  {!s.loggedIn && canLogin && (
                    <span className="ml-2 text-xs font-normal text-slate">未登录</span>
                  )}
                  {!s.loggedIn && !canLogin && (
                    <span className="ml-2 text-xs font-normal text-slate">暂不支持登录</span>
                  )}
                </div>
                <div className="mt-0.5 truncate text-xs text-slate">
                  {s.loggedIn
                    ? s.username
                      ? `当前用户：${s.username}`
                      : '会话已保存（该平台未提供用户名查询）'
                    : !canLogin
                      ? '弹幕/观看可正常使用；账号登录暂无可行方案'
                      : canQr
                        ? '未登录——点击「扫码登录」用平台 App 扫码'
                        : '未登录——点击「内嵌登录」完成登录'}
                </div>
              </div>
              <div className="flex shrink-0 items-center gap-2">
                {/* 启用/停用（6.1）：停用后该平台不参与搜索 */}
                <button
                  onClick={() => togglePlatformEnabled(s.platform)}
                  title={disabledPlatforms.has(s.platform) ? `启用${s.label}` : `停用${s.label}（不参与搜索）`}
                  className={`rounded-lg border px-2.5 py-1.5 text-[10px] transition-colors ${
                    disabledPlatforms.has(s.platform)
                      ? 'border-danger/40 bg-danger/10 text-danger'
                      : 'border-line text-slate hover:border-stream hover:text-stream'
                  }`}
                >
                  {disabledPlatforms.has(s.platform) ? '已停用' : '已启用'}
                </button>
                {/* 连接检测（6.1） */}
                <button
                  onClick={() => handleCheckConn(s.platform)}
                  title={`检测${s.label}门户连通性`}
                  className="rounded-lg border border-line px-2.5 py-1.5 text-[10px] text-slate transition-colors hover:border-stream hover:text-stream"
                >
                  {connResults[s.platform] ?? '检测'}
                </button>
                {/* 关注展示开关：隐藏后该平台关注不出现在关注页/首页 */}
                <button
                  onClick={() => toggleFollowVisible(s.platform)}
                  title={
                    hiddenFollowPlatforms.has(s.platform)
                      ? `展示${s.label}的关注`
                      : `隐藏${s.label}的关注`
                  }
                  className={`rounded-lg border px-2.5 py-1.5 text-[10px] transition-colors ${
                    hiddenFollowPlatforms.has(s.platform)
                      ? 'border-line bg-canvas text-slate hover:border-stream hover:text-stream'
                      : 'border-stream/40 bg-stream/10 text-stream'
                  }`}
                >
                  {hiddenFollowPlatforms.has(s.platform) ? '关注已隐藏' : '展示关注'}
                </button>
                {!s.loggedIn && canQr && canLogin && (
                  <button
                    onClick={() => openQr(s.platform)}
                    disabled={busy === s.platform}
                    className="brand-gradient rounded-lg px-3 py-1.5 text-xs font-medium text-white transition-opacity hover:opacity-90 disabled:opacity-40"
                  >
                    {busy === s.platform ? '生成中…' : '扫码登录'}
                  </button>
                )}
                {!s.loggedIn && !canQr && canLogin && (
                  <>
                    <button
                      onClick={() => handleOpen(s.platform)}
                      disabled={busy === s.platform}
                      className="rounded-lg border border-line px-3 py-1.5 text-xs text-ink transition-colors hover:border-stream hover:text-stream disabled:opacity-40"
                    >
                      内嵌登录
                    </button>
                    <button
                      onClick={() => handleComplete(s.platform)}
                      disabled={busy === s.platform}
                      className="brand-gradient rounded-lg px-3 py-1.5 text-xs font-medium text-white transition-opacity hover:opacity-90 disabled:opacity-40"
                    >
                      {busy === s.platform ? '处理中…' : '完成登录'}
                    </button>
                  </>
                )}
                {s.loggedIn && canLogin && (
                  <button
                    onClick={() => handleLogout(s.platform)}
                    disabled={busy === s.platform}
                    className="rounded-lg border border-line px-3 py-1.5 text-xs text-slate transition-colors hover:border-danger hover:text-danger disabled:opacity-40"
                  >
                    退出登录
                  </button>
                )}
              </div>
            </div>
          );
        })}
      </div>

      <div className="rounded-card border border-line bg-surface px-5 py-4">
        <h3 className="text-sm font-semibold text-ink">外观</h3>
        <p className="mt-1 text-xs text-slate">
          主题模式（当前{effectiveTheme === 'dark' ? '深色' : '浅色'}）
        </p>
        <div className="mt-3 grid grid-cols-3 gap-2">
          {(
            [
              { key: 'system', label: '跟随系统', desc: '随 Windows 设置' },
              { key: 'light', label: '浅色', desc: '明亮清爽' },
              { key: 'dark', label: '深色', desc: '沉浸观看' },
            ] as const
          ).map((opt) => (
            <button
              key={opt.key}
              onClick={() => setTheme(opt.key)}
              className={`rounded-lg border px-3 py-2.5 text-left transition-colors ${
                theme === opt.key
                  ? 'border-stream bg-stream/10 text-stream'
                  : 'border-line text-slate hover:border-stream/60 hover:text-ink'
              }`}
            >
              <div className="text-xs font-medium">{opt.label}</div>
              <div className="mt-0.5 text-[10px] opacity-80">{opt.desc}</div>
            </button>
          ))}
        </div>
      </div>

      <div className="rounded-card border border-line bg-surface px-5 py-4">
        <h3 className="text-sm font-semibold text-ink">关于登录与隐私</h3>
        <ul className="mt-2 list-disc space-y-1 pl-5 text-xs text-slate">
          <li>登录在平台官方页面/官方扫码接口完成，StreamFlow 不接触你的账号密码。</li>
          <li>登录产生的会话 cookie 仅保存在本机应用数据目录，用于访问关注列表等功能。</li>
          <li>退出登录会立即清除本机会话；在平台官网改密码/退出登录也会使其失效。</li>
        </ul>
      </div>

      <div className="rounded-card border border-line bg-surface px-5 py-4">
        <h3 className="text-sm font-semibold text-ink">数据与隐私</h3>
        <p className="mt-1 text-xs text-slate">
          登录态（各平台会话 cookie）可导出为文件，在新设备导入后无需重复登录。
          文件为明文 JSON，请妥善保管，不要分享给他人；平台侧会话过期后仍需重新登录。
        </p>
        <div className="mt-3 flex items-center gap-2">
          <button
            onClick={handleExportAuth}
            className="brand-gradient rounded-lg px-4 py-2 text-xs font-medium text-white transition-opacity hover:opacity-90"
          >
            导出登录态
          </button>
          <button
            onClick={() => importInputRef.current?.click()}
            disabled={busy === 'import'}
            className="rounded-lg border border-line px-4 py-2 text-xs text-ink transition-colors hover:border-stream hover:text-stream disabled:opacity-40"
          >
            {busy === 'import' ? '导入中…' : '导入登录态'}
          </button>
          <input
            ref={importInputRef}
            type="file"
            accept="application/json,.json"
            className="hidden"
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (file) void handleImportAuth(file);
              e.target.value = '';
            }}
          />
          <button
            onClick={handleClearCache}
            disabled={busy === 'cache'}
            className="rounded-lg border border-danger/40 px-4 py-2 text-xs text-danger transition-colors hover:bg-danger/10 disabled:opacity-40"
          >
            {busy === 'cache' ? '清理中…' : '清理网页缓存'}
          </button>
        </div>
      </div>

      <div className="rounded-card border border-line bg-surface px-5 py-4">
        <h3 className="text-sm font-semibold text-ink">播放设置</h3>
        <p className="mt-1 text-xs text-slate">
          音量会自动记忆（调整音量后下次进入直播间自动恢复）。
        </p>
        <div className="mt-3 flex items-center gap-6">
          <label className="flex cursor-pointer items-center gap-2 text-xs text-ink">
            <input
              type="checkbox"
              checked={defaultMuted}
              onChange={(e) => handleDefaultMuted(e.target.checked)}
              className="h-3.5 w-3.5"
            />
            进入直播间默认静音
          </label>
          <div className="flex items-center gap-2 text-xs">
            <span className="text-slate">卡片点击默认</span>
            {(
              [
                { key: 'internal', label: '站内播放' },
                { key: 'external', label: '外部浏览器打开' },
              ] as const
            ).map((opt) => (
              <button
                key={opt.key}
                onClick={() => handleDefaultOpenMode(opt.key)}
                className={`rounded-lg border px-3 py-1.5 text-[10px] transition-colors ${
                  defaultOpenMode === opt.key
                    ? 'border-stream bg-stream/10 text-stream'
                    : 'border-line text-slate hover:border-stream/60 hover:text-ink'
                }`}
              >
                {opt.label}
              </button>
            ))}
          </div>
        </div>
      </div>

      <div className="rounded-card border border-line bg-surface px-5 py-4">
        <h3 className="text-sm font-semibold text-ink">关于</h3>
        <ul className="mt-2 list-disc space-y-1 pl-5 text-xs text-slate">
          <li>StreamFlow 桌面版 v{appVersion}（Tauri 2 + React）。</li>
          <li>收藏/历史/设置存于本机 SQLite（应用数据目录 streamflow.db）。</li>
          <li>弹幕均为只读接入，不发送、不注入任何平台。</li>
        </ul>
        <div className="mt-3 flex items-center gap-2">
          <button
            onClick={handleCheckUpdate}
            disabled={updateState.kind === 'checking' || updateState.kind === 'downloading'}
            className="rounded-lg border border-line px-4 py-2 text-xs text-ink transition-colors hover:border-stream hover:text-stream disabled:opacity-40"
          >
            {updateState.kind === 'checking'
              ? '检查中…'
              : updateState.kind === 'downloading'
                ? '下载安装中…'
                : '检查更新'}
          </button>
          {updateState.kind === 'latest' && (
            <span className="text-xs text-success">已是最新版本</span>
          )}
          {updateState.kind === 'available' && (
            <>
              <span className="text-xs text-stream">发现新版本 v{updateState.version}</span>
              <button
                onClick={handleInstallUpdate}
                className="brand-gradient rounded-lg px-3 py-2 text-xs font-medium text-white transition-opacity hover:opacity-90"
              >
                下载并安装
              </button>
            </>
          )}
          {updateState.kind === 'error' && (
            <span className="text-xs text-danger">检查失败：{updateState.message.slice(0, 60)}</span>
          )}
        </div>
        {updateState.kind === 'available' && updateState.notes && (
          <p className="mt-2 whitespace-pre-wrap rounded-lg border border-line bg-canvas px-3 py-2 text-xs text-slate">
            {updateState.notes.slice(0, 300)}
          </p>
        )}
        <p className="mt-2 text-[10px] text-slate">
          更新源：GitHub Releases（安装包经签名验证后自动安装并重启）。
        </p>
      </div>

      {qr && (
        <div
          className="fixed inset-0 z-50 grid place-items-center bg-black/60 p-6"
          onClick={() => setQr(null)}
        >
          <div
            className="w-[320px] rounded-2xl border border-line bg-surface p-6 text-center shadow-xl"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="text-sm font-semibold text-ink">
              {labelOf(qr.platform)}扫码登录
            </div>
            <div className="mx-auto mt-4 h-[220px] w-[220px] [&_svg]:h-full [&_svg]:w-full">
              {qr.status === 'expired' ? (
                <div className="grid h-full place-items-center rounded-xl border border-line bg-canvas text-xs text-slate">
                  二维码已过期
                </div>
              ) : (
                <div dangerouslySetInnerHTML={{ __html: qr.svg }} />
              )}
            </div>
            <p
              className={`mt-4 text-xs ${
                qr.error
                  ? 'text-danger'
                  : qr.status === 'success'
                    ? 'text-success'
                    : qr.status === 'scanned'
                      ? 'text-stream'
                      : 'text-slate'
              }`}
            >
              {qr.error ?? QR_STATUS_TEXT[qr.status]}
            </p>
            <div className="mt-4 flex items-center justify-center gap-2">
              {qr.status === 'expired' && (
                <button
                  onClick={() => openQr(qr.platform)}
                  className="brand-gradient rounded-lg px-4 py-1.5 text-xs font-medium text-white hover:opacity-90"
                >
                  刷新二维码
                </button>
              )}
              <button
                onClick={() => setQr(null)}
                className="rounded-lg border border-line px-4 py-1.5 text-xs text-slate hover:border-slate hover:text-ink"
              >
                关闭
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
