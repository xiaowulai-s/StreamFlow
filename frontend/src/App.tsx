import { useEffect, useRef, useState } from 'react'
import Hls from 'hls.js'
import mpegts from 'mpegts.js'
import './App.css'

type Platform = 'bilibili' | 'douyu' | 'huya' | 'douyin' | 'kuaishou';

interface Room {
  id: string;
  platform: Platform;
  title: string;
  username: string;
  viewCount: number;
  status: 'live' | 'sleeping' | 'error';
}

function App() {
  const [rooms, setRooms] = useState<Room[]>([]);
  const [selectedPlatform, setSelectedPlatform] = useState<Platform>('bilibili');
  const [searchKeyword, setSearchKeyword] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  // 播放器状态：广告过滤原则——仅渲染直播画面，无礼物/横幅等附加 UI
  const [playingRoom, setPlayingRoom] = useState<Room | null>(null);
  const [streamUrl, setStreamUrl] = useState<string | null>(null);
  const videoRef = useRef<HTMLVideoElement>(null);

  const platforms: Platform[] = ['bilibili', 'douyu', 'huya', 'douyin', 'kuaishou'];

  // m3u8 走 hls.js（MSE），flv 走 mpegts.js（斗鱼）；Safari 内核 m3u8 可原生播放
  useEffect(() => {
    const video = videoRef.current;
    if (!streamUrl || !video) return;

    if (streamUrl.includes('.flv')) {
      if (!mpegts.getFeatureList().mseLivePlayback) return;
      const player = mpegts.createPlayer(
        { type: 'flv', isLive: true, url: streamUrl },
        { enableStashBuffer: false }
      );
      player.attachMediaElement(video);
      player.load();
      return () => {
        try { player.destroy(); } catch { /* 忽略销毁异常 */ }
      };
    }

    if (Hls.isSupported()) {
      const hls = new Hls();
      hls.loadSource(streamUrl);
      hls.attachMedia(video);
      return () => hls.destroy();
    }
    if (video.canPlayType('application/vnd.apple.mpegurl')) {
      video.src = streamUrl;
    }
  }, [streamUrl]);

  const handleSearch = async () => {
    if (!searchKeyword.trim()) return;

    setIsLoading(true);

    // 这里会调用 Tauri 后端的 API（Tauri 2 全局 API 位于 core 命名空间）
    try {
      const result = await (window as any).__TAURI__.core.invoke('search_rooms', {
        platform: selectedPlatform,
        keyword: searchKeyword
      });
      // 平台搜索接口不可用时（虎牙/斗鱼），关键词可作为房间号直达观看
      const list: Room[] = Array.isArray(result) && result.length > 0
        ? result
        : [{ id: searchKeyword.trim(), platform: selectedPlatform, title: `房间 ${searchKeyword.trim()}`, username: '', viewCount: 0, status: 'sleeping' }];
      setRooms(list);
    } catch (error) {
      console.error('Search failed:', error);
    } finally {
      setIsLoading(false);
    }
  };

  const handleWatch = async (room: Room) => {
    try {
      const result = await (window as any).__TAURI__.core.invoke('get_stream_urls', {
        platform: room.platform,
        roomId: room.id
      });
      const urls: string[] = Array.isArray(result?.url) ? result.url : [];
      if (urls.length > 0) {
        setPlayingRoom(room);
        setStreamUrl(urls[0]);
      } else {
        alert('未获取到直播流地址');
      }
    } catch (err) {
      // ExternalOnly 兜底：提示手动打开外链
      const msg = String(err);
      const m = msg.match(/https?:\/\/[^"\s}]+/);
      alert(m ? `该平台暂不支持内嵌播放，请手动打开：\n${m[0]}` : `获取直播流失败：${msg}`);
    }
  };

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-gray-900">
      {/* Header */}
      <header className="sticky top-0 bg-white/80 dark:bg-gray-800/80 backdrop-blur-sm border-b border-gray-200 dark:border-gray-700 z-10">
        <div className="max-w-6xl mx-auto px-4 py-3 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 bg-gradient-to-r from-blue-500 to-cyan-400 rounded-lg flex items-center justify-center">
              <span className="text-white font-bold text-xl">S</span>
            </div>
            <h1 className="text-xl font-bold text-gray-800 dark:text-white">StreamFlow</h1>
          </div>
          <div className="text-sm text-gray-500 dark:text-gray-400">
            Multi-platform Live Stream Aggregator
          </div>
        </div>
      </header>

      {/* Main Content */}
      <main className="max-w-6xl mx-auto px-4 py-6">
        {/* Search Controls */}
        <div className="bg-white dark:bg-gray-800 rounded-xl shadow-sm border border-gray-200 dark:border-gray-700 p-4 mb-6">
          <div className="flex gap-3 items-start">
            <select
              value={selectedPlatform}
              onChange={(e) => setSelectedPlatform(e.target.value as Platform)}
              className="px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-sm font-medium"
            >
              {platforms.map(p => (
                <option key={p} value={p}>
                  {p.charAt(0).toUpperCase() + p.slice(1)}
                </option>
              ))}
            </select>
            
            <input
              type="text"
              placeholder="搜索直播间..."
              value={searchKeyword}
              onChange={(e) => setSearchKeyword(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && handleSearch()}
              className="flex-1 px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg focus:ring-2 focus:ring-blue-500 bg-white dark:bg-gray-700"
            />
            
            <button
              onClick={handleSearch}
              disabled={isLoading || !searchKeyword.trim()}
              className="px-6 py-2 bg-gradient-to-r from-blue-500 to-cyan-400 text-white rounded-lg font-medium hover:opacity-90 disabled:opacity-50 transition-colors"
            >
              {isLoading ? '搜索中...' : '搜索'}
            </button>
          </div>
        </div>

        {/* Rooms List */}
        <div className="space-y-4">
          {rooms.length === 0 ? (
            <div className="text-center py-12">
              <div className="text-gray-400 text-lg">暂无直播间</div>
              <p className="text-gray-500 dark:text-gray-400 mt-2">输入关键词开始搜索</p>
            </div>
          ) : (
            rooms.map(room => (
              <div
                key={room.id}
                className="bg-white dark:bg-gray-800 rounded-xl shadow-sm border border-gray-200 dark:border-gray-700 p-4 hover:shadow-md transition-shadow"
              >
                <div className="flex items-center gap-4">
                  <div className="w-16 h-16 bg-gradient-to-r from-blue-500 to-cyan-400 rounded-lg flex items-center justify-center text-white font-bold text-lg">
                    {(room.username || '?').charAt(0)}
                  </div>
                  
                  <div className="flex-1">
                    <h3 className="font-semibold text-gray-800 dark:text-white">{room.title}</h3>
                    <p className="text-sm text-gray-500 dark:text-gray-400">{room.username}</p>
                    <div className="flex items-center gap-2 mt-2">
                      <span className="px-2 py-1 text-xs font-medium rounded-full bg-green-100 text-green-600">
                        {room.status === 'live' ? '直播中' : '等待中'}
                      </span>
                      <span className="text-xs text-gray-400">{room.viewCount.toLocaleString()} 观看人数</span>
                    </div>
                  </div>
                  
                  <button
                    onClick={() => handleWatch(room)}
                    className="px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition-colors"
                  >
                    观看
                  </button>
                </div>
              </div>
            ))
          )}
        </div>
      </main>

      {/* 播放器弹层：广告过滤设计——画面仅为原生直播流，无任何礼物/广告元素 */}
      {playingRoom && (
        <div
          className="fixed inset-0 z-50 bg-black/80 flex items-center justify-center p-4"
          onClick={() => { setPlayingRoom(null); setStreamUrl(null); }}
        >
          <div
            className="bg-gray-900 rounded-xl w-full max-w-4xl p-4"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex justify-between items-center mb-3">
              <h3 className="text-white font-semibold">{playingRoom.title}</h3>
              <button
                onClick={() => { setPlayingRoom(null); setStreamUrl(null); }}
                className="px-3 py-1 text-sm text-gray-300 hover:text-white border border-gray-600 rounded-lg transition-colors"
              >
                关闭
              </button>
            </div>
            <video
              ref={videoRef}
              controls
              autoPlay
              className="w-full aspect-video bg-black rounded-lg"
            />
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
