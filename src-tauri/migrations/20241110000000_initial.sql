-- Initial migration for StreamFlow
-- Create tables: rooms, history, settings

-- 房间信息表
CREATE TABLE IF NOT EXISTS rooms (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    platform TEXT NOT NULL,
    room_id TEXT NOT NULL,
    title TEXT NOT NULL,
    username TEXT NOT NULL,
    view_count INTEGER DEFAULT 0,
    status TEXT DEFAULT 'sleeping',
    last_checked TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(platform, room_id)
);

-- 观看历史表
CREATE TABLE IF NOT EXISTS history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    room_id TEXT NOT NULL,
    platform TEXT NOT NULL,
    watched_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    duration_sec INTEGER DEFAULT 0
);

-- 设置表
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- 创建索引提高查询性能
CREATE INDEX IF NOT EXISTS idx_rooms_platform ON rooms(platform);
CREATE INDEX IF NOT EXISTS idx_rooms_status ON rooms(status);
CREATE INDEX IF NOT EXISTS idx_history_room ON history(room_id);
CREATE INDEX IF NOT EXISTS idx_history_watched ON history(watched_at);
