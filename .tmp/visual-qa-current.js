const puppeteer = require('puppeteer-core');
const path = require('path');
const { pathToFileURL } = require('url');

const browserPath = 'C:/Program Files (x86)/Google/Chrome/Application/chrome.exe';
const pageUrl = pathToFileURL(path.resolve('windows/ui/dashboard/index.html')).href;
const today = new Date().toISOString().slice(0, 10);
const dayKey = (offset) => new Date(Date.now() - offset * 86400000).toISOString().slice(0, 10);
const tools = { Codex: 646521, 'Claude Code': 318214, Cursor: 209201, Copilot: 119433, 其他: 49311 };
const models = { 'gpt-4o': 565432, 'claude-3.5-sonnet': 326651, 'qwen-max': 211872, 'claude-3-opus': 130547, 其他模型: 108178 };
const daily = Array.from({ length: 30 }, (_, index) => ({
  date: dayKey(29 - index),
  total_tokens: index === 29 ? 1342680 : 520000 + ((index * 173421) % 1450000),
  cost: index === 29 ? 18.72 : 8 + ((index * 3.7) % 22),
  tools: index === 29 ? tools : { Codex: 300000 + index * 1200, 'Claude Code': 150000 + index * 700 },
  models: index === 29 ? models : { 'gpt-4o': 310000 + index * 1000, 'claude-3.5-sonnet': 120000 + index * 800 }
}));
daily[29].date = today;
if (process.argv.includes('--overlap')) daily[29].total_tokens = 2500000;
const snapshot = {
  totals: { tokens: 156780320, cost: 512.38, active_days: 48 },
  daily,
  rhythms: [{ date: today, buckets: Array.from({ length: 24 }, (_, hour) => ({ hour, tokens: 38000 + ((hour * 17321) % 100000) })), peak_hour: 8 }],
  agent_work: [{ date: today, total_tokens: 1842120, input_tokens: 2316540, cached_input_tokens: 1575250, output_tokens: 482140, cache_coverage_complete: true, model_request_count: 12580, tool_call_count: 3420, active_hours: 7.4 }],
  tools,
  models,
  sources: {}
};
const settings = { theme: 'green', language: 'zhHans', daily_goal_tokens: 2000000, monthly_budget: 1500, refresh_interval_seconds: 60, history_days: 180, agent_work_rank_visibility: 'visible', close_to_tray: true };
const quota = { providers: [
  { provider: 'openai', status: 'available', windows: [{ kind: 'monthly_credits', title: '本月', used_percent: 70, total: 100, remaining: 30 }] },
  { provider: 'anthropic', status: 'available', windows: [{ kind: 'monthly_credits', title: '本月', used_percent: 58, total: 100, remaining: 42 }] },
  { provider: 'google', status: 'available', windows: [{ kind: 'monthly_credits', title: '本月', used_percent: 36, total: 100, remaining: 64 }] },
  { provider: 'local', status: 'available', windows: [{ kind: 'local', title: '本地运行', used_percent: 0, total: 0, remaining: 0 }] }
] };

(async () => {
  const browser = await puppeteer.launch({ headless: true, executablePath: browserPath, args: ['--no-sandbox', '--disable-gpu', '--allow-file-access-from-files'] });
  const page = await browser.newPage();
  await page.setViewport({ width: 1536, height: 1024, deviceScaleFactor: 1 });
  page.setDefaultTimeout(5000);
  page.on('console', (message) => console.log(`[console:${message.type()}] ${message.text()}`));
  page.on('pageerror', (error) => console.log(`[pageerror] ${error.message}`));
  await page.evaluateOnNewDocument((settingsArg, snapshotArg, quotaArg) => {
    window.__TAURI__ = { core: { invoke: async (cmd) => {
      if (cmd === 'get_settings') return settingsArg;
      if (cmd === 'get_snapshot') return snapshotArg;
      if (cmd === 'read_quota_providers') return quotaArg;
      if (cmd === 'read_token_rank') return { available: true, identity: { id: 2, name: '翻译助手' }, top: { rank: 1, user_id: 1, name: '代码助手', total_tokens: 482140 }, mine: { rank: 2, user_id: 2, name: '翻译助手', total_tokens: 286530 }, total_ranked_users: 5, total_tokens: 946990 };
      if (cmd === 'check_for_update') return { has_update: false };
      if (cmd === 'get_recalibration_notice') return false;
      if (cmd === 'app_version') return '0.1.9';
      return {};
    } }, event: { listen: async () => () => {} } };
  }, settings, snapshot, quota);
  await page.goto(pageUrl, { waitUntil: 'domcontentloaded', timeout: 8000 });
  await new Promise((resolve) => setTimeout(resolve, 1200));
  const result = await page.evaluate(() => ({
    title: document.querySelector('.reference-brand-title')?.textContent,
    page: Boolean(document.querySelector('.today-design-page')),
    cards: document.querySelectorAll('.today-design-card').length,
    ringText: document.querySelector('.today-ring-center strong')?.textContent,
    currentLapText: document.querySelector('.today-lap-chips > div:last-child strong')?.textContent,
    lapAboveOne: window.TS.lapProgress(2500000, 2000000),
    top: document.querySelector('.today-design-top')?.getBoundingClientRect().toJSON(),
    metrics: document.querySelector('.today-design-metrics')?.getBoundingClientRect().toJSON(),
    middle: document.querySelector('.today-design-middle')?.getBoundingClientRect().toJSON(),
    bottom: document.querySelector('.today-design-bottom')?.getBoundingClientRect().toJSON()
  }));
  console.log(JSON.stringify(result, null, 2));
  await page.screenshot({ path: path.resolve('.tmp/today-current-mock.png'), fullPage: true });
  await browser.close();
})();
