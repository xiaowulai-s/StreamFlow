import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'
import { applyCachedTheme } from './lib/theme'

// 首屏先按缓存/系统应用主题，避免亮暗闪烁；随后由 SQLite 设置校正
applyCachedTheme()

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
