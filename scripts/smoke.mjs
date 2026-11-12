#!/usr/bin/env node
/**
 * StreamFlow 回归冒烟测试
 *
 * 前置：应用以远程调试端口启动
 *   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9333" ./streamflow.exe
 *
 * 运行：node scripts/smoke.mjs [PORT]
 * 检查项：版本读取 / 五平台健康 / 五平台搜索 / 设置读写 / 关注与历史 API
 */
const PORT = process.argv[2] || '9333';
const list = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
const page = list.find((t) => t.type === 'page' && t.url.includes('tauri.localhost'));
if (!page) {
  console.error('✗ 主窗口未就绪（确认应用已带 --remote-debugging-port=9333 启动）');
  process.exit(1);
}
const ws = new WebSocket(page.webSocketDebuggerUrl);
let seq = 0;
const pending = new Map();
ws.onmessage = (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id && pending.has(m.id)) {
    const p = pending.get(m.id);
    pending.delete(m.id);
    m.error ? p.reject(new Error(JSON.stringify(m.error))) : p.resolve(m.result);
  }
};
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
const send = (method, params = {}) =>
  new Promise((r, j) => { const id = ++seq; pending.set(id, { resolve: r, reject: j }); ws.send(JSON.stringify({ id, method, params })); });
const evalJs = async (expression) => {
  const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description || 'eval failed');
  return r.result.value;
};

let pass = 0, fail = 0;
const check = (name, ok, detail = '') => {
  console.log(`${ok ? '✓' : '✗'} ${name}${detail ? ` — ${detail}` : ''}`);
  ok ? pass++ : fail++;
};

// 1. 版本
try {
  const v = await evalJs('window.__TAURI__.app.getVersion()');
  check('版本读取', /^\d+\.\d+\.\d+$/.test(v), `v${v}`);
} catch (e) { check('版本读取', false, String(e).slice(0, 60)); }

// 2. 五平台健康（门户 + 接口；抖音/快手当前已知风控失效，降级为警告）
try {
  const health = await evalJs(`Promise.all(["huya","douyu","bilibili","douyin","kuaishou"].map(p=>
    window.__TAURI__.core.invoke("check_platform_health",{platform:p}).then(h=>({p,...h})).catch(e=>({p,apiOk:false,apiDetail:String(e).slice(0,40),portalMs:null}))))`);
  for (const h of health) {
    const core = ['huya', 'douyu', 'bilibili'].includes(h.p);
    const ok = h.portalMs != null && (h.apiOk || !core);
    check(`健康[${h.p}]`, ok, `门户=${h.portalMs ?? '失败'} 接口=${h.apiOk ? `正常(${h.roomCount})` : h.apiDetail}`);
  }
} catch (e) { check('健康检测', false, String(e).slice(0, 60)); }

// 3. 五平台搜索（核心三平台必须非空，抖音/快手允许为空）
try {
  const results = await evalJs(`Promise.all(["huya","douyu","bilibili","douyin","kuaishou"].map(p=>
    window.__TAURI__.core.invoke("search_rooms",{platform:p,keyword:"直播"}).then(v=>({p,n:v.length})).catch(e=>({p,n:-1,err:String(e).slice(0,30)}))))`);
  for (const r of results) {
    const core = ['huya', 'douyu', 'bilibili'].includes(r.p);
    check(`搜索[${r.p}]`, core ? r.n > 0 : r.n >= 0, r.err ? `失败:${r.err}` : `${r.n} 条`);
  }
} catch (e) { check('搜索', false, String(e).slice(0, 60)); }

// 4. 设置读写往返
try {
  const round = await evalJs(`window.__TAURI__.core.invoke("set_setting",{key:"__smoke_test__",value:"ok"})
    .then(()=>window.__TAURI__.core.invoke("get_setting",{key:"__smoke_test__"}))
    .then(v=>window.__TAURI__.core.invoke("set_setting",{key:"__smoke_test__",value:""}).then(()=>v))`);
  check('设置读写', round === 'ok', `回读=${round}`);
} catch (e) { check('设置读写', false, String(e).slice(0, 60)); }

// 5. 关注/历史 API
try {
  const fav = await evalJs('window.__TAURI__.core.invoke("load_favorites").then(v=>v.length)');
  check('收藏读取', fav >= 0, `${fav} 条`);
  const his = await evalJs('window.__TAURI__.core.invoke("load_history").then(v=>v.length)');
  check('历史读取', his >= 0, `${his} 条`);
} catch (e) { check('收藏/历史', false, String(e).slice(0, 60)); }

console.log(`\n结果：${pass} 通过 / ${fail} 失败`);
ws.close();
process.exit(fail > 0 ? 1 : 0);
