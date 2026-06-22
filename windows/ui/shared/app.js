/* TokenStep Windows — shared frontend helpers.
   Formatters ported from Formatters.swift; contribution color from Components.swift. */

// ---- Translation helpers (source-of-truth translation: every user-facing
// string passes through these at the moment it is generated, instead of
// relying on the post-render DOM walker in applyLanguage — which misses any
// text written to the DOM AFTER it runs (async quota cards, dynamic titles,
// the update modal, variable-concatenated templates). Reads the I18N table +
// current lang that i18n.js exposes as globals. ----
function t(zh) {
  const lang = window.__tsLang || 'zhHans';
  if (!lang || lang === 'zhHans' || !window.I18N) return zh;
  const table = window.I18N[lang] || {};
  return table[zh] || zh;
}
// printf-style translation for templates with variables. The Chinese key uses
// %d (integer) / %@ (anything) placeholders; the translated value keeps the
// same placeholders and we substitute in order.
//   tf('第 %d 圈', 3)        → en 'Lap %d' → 'Lap 3'
//   tf('TokenStep %@ 可用', '0.1.3') → en 'TokenStep %@ Available' → 'TokenStep 0.1.3 Available'
function tf(zh) {
  var lang = window.__tsLang || 'zhHans';
  var s = zh;
  if (lang && lang !== 'zhHans' && window.I18N) {
    var table = window.I18N[lang] || {};
    s = table[zh] || zh;
  }
  var args = Array.prototype.slice.call(arguments, 1);
  var i = 0;
  return s.replace(/%[ds@]/g, function () { return String(args[i++]); });
}

// ---- Tauri invoke (graceful fallback when running outside Tauri) ----
async function invoke(cmd, args) {
  if (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke) {
    return window.__TAURI__.core.invoke(cmd, args);
  }
  if (window.__TAURI_INTERNALS__) {
    return window.__TAURI_INTERNALS__.invoke(cmd, args);
  }
  console.warn("Tauri invoke unavailable; returning mock for", cmd);
  return null;
}

function listen(event, handler) {
  if (window.__TAURI__ && window.__TAURI__.event && window.__TAURI__.event.listen) {
    return window.__TAURI__.event.listen(event, handler);
  }
  return Promise.resolve(() => {});
}

// ---- Formatters (mirror Formatters.swift) ----
function trimTrailing(value, digits) {
  let text = value.toFixed(digits);
  text = text.replace(/\.0+$/, "");
  text = text.replace(/(\.\d*?)0+$/, "$1");
  return text;
}

function formatTokens(value, compact = false) {
  value = Number(value || 0);
  const lang = window.__tsLang || 'zhHans';
  // Unit scales + glyphs differ by language:
  //   en     → K/M/B  (10^3/10^6/10^9)
  //   zhHans → 万/亿  (10^4/10^8)
  //   zhHant → 萬/億  (10^4/10^8, traditional glyphs)
  if (lang === 'en') {
    if (value >= 1000000000) return trimTrailing(value / 1000000000, 2) + "B";
    if (value >= 1000000) {
      const digits = compact || value >= 10000000 ? 1 : 2;
      return trimTrailing(value / 1000000, digits) + "M";
    }
    if (value >= 1000) {
      const digits = compact || value >= 100000 ? 0 : 1;
      return trimTrailing(value / 1000, digits) + "K";
    }
    return String(Math.round(value));
  }
  const wan = lang === 'zhHant' ? "萬" : "万";
  const yi = lang === 'zhHant' ? "億" : "亿";
  if (value >= 100000000) return trimTrailing(value / 100000000, 2) + yi;
  if (value >= 10000) {
    const digits = compact || value >= 10000000 ? 0 : 1;
    return trimTrailing(value / 10000, digits) + wan;
  }
  return String(Math.round(value));
}

function formatMoney(value) {
  const n = Number(value || 0);
  return (
    "$" +
    n.toLocaleString(undefined, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    })
  );
}

function formatPercent(value) {
  value = Number(value || 0);
  if (value >= 100) return Math.round(value) + "%";
  if (value >= 10) return value.toFixed(1) + "%";
  return Math.round(value) + "%";
}

function formatGeneratedTime(value) {
  if (!value) return t("等待同步");
  return value.replace("T", " ").slice(0, 16);
}

function formatInterval(seconds) {
  seconds = Number(seconds || 0);
  if (seconds === 0) return t("手动");
  if (seconds === 60) return t("1 分钟");
  return t("%d 分钟").replace("%d", seconds / 60);
}

// ---- Contribution color (port of Components.swift contributionColor / activityColor) ----
// 4-level GitHub-style activity scale (activity1-4), matching macOS.
function contributionColor(tokens, goal) {
  if (!(tokens > 0)) return themeColors.track;
  const progress = Math.min(tokens / Math.max(goal, 1), 1);
  if (progress >= 0.65) return themeColors.activity4;
  if (progress >= 0.35) return themeColors.activity3;
  if (progress >= 0.12) return themeColors.activity2;
  return themeColors.activity1;
}

// ---- Day key for "today" in Asia/Shanghai ----
function todayKey() {
  // +08:00 fixed offset (matches the collector's timezone).
  const now = new Date();
  const utcMs = now.getTime() + now.getTimezoneOffset() * 60000;
  const shanghai = new Date(utcMs + 8 * 3600000);
  const y = shanghai.getFullYear();
  const m = String(shanghai.getMonth() + 1).padStart(2, "0");
  const d = String(shanghai.getDate()).padStart(2, "0");
  return `${y}-${m}-${d}`;
}

// ---- SVG progress ring ----
// Uses themeColors (kept in sync by applyTheme) so it recolors on theme switch.
// SVG attributes like stop-color can't use CSS var(), so we need raw values.
function ringSvg(progress, size = 148, stroke = 16) {
  const r = (size - stroke) / 2;
  const circ = 2 * Math.PI * r;
  const clamped = Math.max(0, Math.min(progress, 1));
  const offset = circ * (1 - clamped);
  const mint = themeColors.mint;
  const green = themeColors.green;
  const greenDark = themeColors.greenDark;
  const track = themeColors.track;
  const gradId = "ringGrad_" + Math.random().toString(36).slice(2, 8);
  return `
    <svg width="${size}" height="${size}" viewBox="0 0 ${size} ${size}">
      <defs>
        <linearGradient id="${gradId}" x1="0%" y1="100%" x2="100%" y2="0%">
          <stop offset="0%" stop-color="${mint}"/>
          <stop offset="50%" stop-color="${green}"/>
          <stop offset="100%" stop-color="${greenDark}"/>
        </linearGradient>
      </defs>
      <circle cx="${size / 2}" cy="${size / 2}" r="${r}"
        fill="none" stroke="${track}" stroke-width="${stroke}" stroke-linecap="round"/>
      <circle cx="${size / 2}" cy="${size / 2}" r="${r}"
        fill="none" stroke="url(#${gradId})" stroke-width="${stroke}" stroke-linecap="round"
        stroke-dasharray="${circ}" stroke-dashoffset="${offset}"
        transform="rotate(-90 ${size / 2} ${size / 2})"/>
    </svg>`;
}

// ---- Simple SVG icons (no external icon font needed) ----
const ICONS = {
  walk: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><circle cx="13" cy="4" r="2"/><path d="M9 20l2-6 3 2 1 4M9 20l-2-4M11 14l-1-4 4-2 2 3 3 1"/></svg>',
  grid: '<svg viewBox="0 0 24 24" fill="currentColor"><rect x="3" y="3" width="6" height="6" rx="1.5"/><rect x="15" y="3" width="6" height="6" rx="1.5"/><rect x="3" y="15" width="6" height="6" rx="1.5"/><rect x="15" y="15" width="6" height="6" rx="1.5"/></svg>',
  chart: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><line x1="3" y1="20" x2="21" y2="20"/><rect x="5" y="11" width="3" height="6" rx="1"/><rect x="10.5" y="7" width="3" height="10" rx="1"/><rect x="16" y="13" width="3" height="4" rx="1"/></svg>',
  shield: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6l8-3z"/><path d="M9 12l2 2 4-4"/></svg>',
  refresh: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 1 1-3-6.7L21 8"/><path d="M21 3v5h-5"/></svg>',
  gear: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1-1.5 1.7 1.7 0 0 0-1.9.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.9 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1 1.7 1.7 0 0 0-.3-1.9l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.9.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.9-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.9V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"/></svg>',
};

// ---- Lap-based progress (port of LapProgress.swift) ----
// Tokens are grouped into "laps" of `goal` size. Each completed lap wraps the
// ring back to 0, so a high-usage day shows multiple laps.
function lapProgress(tokens, goal) {
  const safeGoal = Math.max(goal, 1);
  const t = Math.max(0, tokens);
  const completedLaps = Math.floor(t / safeGoal);
  const remainder = t % safeGoal;
  const currentLap = t > 0 ? Math.max(1, completedLaps + (remainder > 0 ? 1 : 0)) : 1;
  const currentLapProgress = t > 0 ? (remainder === 0 ? 1 : remainder / safeGoal) : 0;
  // Lap color cycles through progressively darker shades (theme-aware).
  const lapColors = [themeColors.ring1, themeColors.ring2, themeColors.ring3, themeColors.ring4];
  return {
    completedLaps,
    currentLap,
    currentLapProgress,
    currentLapPercent: currentLapProgress * 100,
    color: lapColors[Math.min(currentLap, lapColors.length) - 1] || lapColors[lapColors.length - 1],
    lapTitle: tf("第 %d 圈", currentLap),
  };
}

// ---- Screenshot: render a shareable data card as PNG via Canvas ----
// (port of macOS ScreenshotExporter.swift intent — capture the dashboard as
// an image. On the web we can't snapshot the DOM reliably without a heavy
// library, so we draw a clean data card on a canvas instead.)
function renderDataCard(canvas, data) {
  const ctx = canvas.getContext("2d");
  // roundRect polyfill for older WebView2.
  if (!CanvasRenderingContext2D.prototype.roundRect) {
    CanvasRenderingContext2D.prototype.roundRect = function (x, y, w, h, r) {
      this.beginPath();
      this.moveTo(x + r, y);
      this.arcTo(x + w, y, x + w, y + h, r);
      this.arcTo(x + w, y + h, x, y + h, r);
      this.arcTo(x, y + h, x, y, r);
      this.arcTo(x, y, x + w, y, r);
      this.closePath();
      return this;
    };
  }
  const W = canvas.width;
  const H = canvas.height;
  const today = data.today || { total_tokens: 0, cost: 0 };
  const settings = data.settings || { daily_goal_tokens: 100000000 };
  const lap = lapProgress(today.total_tokens, settings.daily_goal_tokens);

  // Background gradient (theme-aware, via themeColors cache).
  const C = themeColors;
  const bg = ctx.createLinearGradient(0, 0, W, H);
  bg.addColorStop(0, C.canvas);
  bg.addColorStop(1, C.surface);
  ctx.fillStyle = bg;
  ctx.fillRect(0, 0, W, H);

  // Card panel.
  const pad = 40;
  ctx.fillStyle = C.surface;
  ctx.beginPath();
  ctx.roundRect(pad, pad, W - pad * 2, H - pad * 2, 28);
  ctx.fill();

  // Title.
  ctx.fillStyle = C.ink;
  ctx.font = "700 32px 'Segoe UI', sans-serif";
  ctx.textAlign = "left";
  ctx.fillText("TokenStep", pad + 36, pad + 60);
  ctx.fillStyle = C.muted;
  ctx.font = "600 16px 'Segoe UI', sans-serif";
  ctx.fillText(t("今日 AI 步数 · ") + (today.date || ""), pad + 36, pad + 88);

  // Progress ring (drawn manually).
  const cx = pad + 110;
  const cy = pad + 200;
  const r = 70;
  ctx.strokeStyle = C.track;
  ctx.lineWidth = 16;
  ctx.beginPath();
  ctx.arc(cx, cy, r, 0, Math.PI * 2);
  ctx.stroke();
  ctx.strokeStyle = lap.color || C.green;
  ctx.beginPath();
  ctx.arc(cx, cy, r, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * lap.currentLapProgress);
  ctx.stroke();
  // Ring center text.
  ctx.fillStyle = C.ink;
  ctx.textAlign = "center";
  ctx.font = "800 28px 'Segoe UI', sans-serif";
  ctx.fillText(formatTokens(today.total_tokens), cx, cy + 4);
  ctx.fillStyle = C.muted;
  ctx.font = "600 13px 'Segoe UI', sans-serif";
  ctx.fillText("/ " + formatTokens(settings.daily_goal_tokens, true), cx, cy + 24);

  // Right-side stats.
  const rx = cx + r + 60;
  ctx.textAlign = "left";
  ctx.fillStyle = lap.color || C.green;
  ctx.font = "800 40px 'Segoe UI', sans-serif";
  ctx.fillText(lap.lapTitle, rx, cy - 10);
  ctx.fillStyle = C.muted;
  ctx.font = "600 16px 'Segoe UI', sans-serif";
  ctx.fillText(t("已完成 ") + lap.completedLaps + t(" 圈 · 本圈 ") + formatPercent(lap.currentLapPercent), rx, cy + 20);
  ctx.fillStyle = C.ink;
  ctx.font = "700 18px 'Segoe UI', sans-serif";
  ctx.fillText(t("消耗金额 ") + formatMoney(today.cost), rx, cy + 50);
  ctx.fillStyle = C.muted;
  ctx.fillText(t("活跃 ") + (data.active_days || 0) + t(" 天 · 累计 ") + formatTokens(data.total_tokens || 0, true), rx, cy + 76);

  // Footer.
  ctx.fillStyle = C.mutedFaint;
  ctx.font = "600 13px 'Segoe UI', sans-serif";
  ctx.textAlign = "center";
  ctx.fillText("TokenStep · " + t("十七°") + " · " + t("本地统计"), W / 2, H - pad - 16);
  return canvas;
}

// ---- Share cards (port of macOS ShareDailyCardView / ShareRhythmCardView) ----
// 600×840 Canvas cards designed for social sharing. Content mirrors the macOS
// version block-for-block (header → medal ring → comparison → breakdown
// panels → trend → footer). The "十七°" signature is preserved.

// Ensure roundRect exists (called from share card renderers too).
function _ensureRoundRect(ctx) {
  if (ctx.roundRect) return;
  ctx.roundRect = function (x, y, w, h, r) {
    this.beginPath();
    this.moveTo(x + r, y);
    this.arcTo(x + w, y, x + w, y + h, r);
    this.arcTo(x + w, y + h, x, y + h, r);
    this.arcTo(x, y + h, x, y, r);
    this.arcTo(x, y, x + w, y, r);
    this.closePath();
    return this;
  };
}

// Build a daily share card (today OR yesterday mode).
//   opts.day         — DailyUsage {date, tools:{}, models:{}, total_tokens, cost}
//   opts.previousDay — DailyUsage (for the "比前一天多/少 X%" comparison) or null
//   opts.daily30     — last 30 DailyUsage rows (for the trend panel)
//   opts.settings    — {daily_goal_tokens}
//   opts.mode        — "today" | "yesterday"
function renderShareDailyCard(canvas, opts) {
  const ctx = canvas.getContext("2d");
  _ensureRoundRect(ctx);
  const C = themeColors;
  const W = canvas.width;
  const H = canvas.height;
  const day = opts.day || { total_tokens: 0, cost: 0, tools: {}, models: {} };
  const settings = opts.settings || { daily_goal_tokens: 100000000 };
  const goal = Math.max(settings.daily_goal_tokens, 1);
  const lap = lapProgress(day.total_tokens, goal);
  const pad = 36;

  // 1) Background.
  const bg = ctx.createLinearGradient(0, 0, W, H);
  bg.addColorStop(0, C.canvas);
  bg.addColorStop(1, C.surface);
  ctx.fillStyle = bg;
  ctx.fillRect(0, 0, W, H);

  // 2) Header: brand + subtitle.
  ctx.fillStyle = C.ink;
  ctx.font = "800 34px 'Segoe UI', sans-serif";
  ctx.textAlign = "left";
  ctx.fillText("TokenStep", pad, pad + 30);
  ctx.fillStyle = C.muted;
  ctx.font = "600 15px 'Segoe UI', sans-serif";
  const subtitle =
    (opts.mode === "yesterday" ? t("昨日 AI 步数 · ") : t("今日 AI 步数 · ")) +
    (day.date || "");
  ctx.fillText(subtitle, pad, pad + 54);

  // 3) Medal ring with glow (centered). 212 visual diameter → r=92.
  const cx = W / 2;
  const cy = pad + 90 + 92;
  const r = 92;
  // Glow: draw the progress arc several times with increasing blur.
  ctx.save();
  ctx.shadowColor = lap.color || C.green;
  ctx.shadowBlur = 28;
  ctx.strokeStyle = lap.color || C.green;
  ctx.lineWidth = 20;
  ctx.lineCap = "round";
  ctx.beginPath();
  ctx.arc(cx, cy, r, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * lap.currentLapProgress);
  ctx.stroke();
  ctx.restore();
  // Track ring.
  ctx.strokeStyle = C.track;
  ctx.lineWidth = 20;
  ctx.beginPath();
  ctx.arc(cx, cy, r, 0, Math.PI * 2);
  ctx.stroke();
  // Re-stroke progress on top (without glow) for crisp edge.
  ctx.strokeStyle = lap.color || C.green;
  ctx.lineWidth = 20;
  ctx.lineCap = "round";
  ctx.beginPath();
  ctx.arc(cx, cy, r, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * lap.currentLapProgress);
  ctx.stroke();
  // Center: completion % + lap line.
  const completionPct = Math.min(100, Math.round((day.total_tokens / goal) * 100));
  ctx.fillStyle = C.ink;
  ctx.textAlign = "center";
  ctx.font = "800 40px 'Segoe UI', sans-serif";
  ctx.fillText(completionPct + "%", cx, cy + 2);
  ctx.fillStyle = C.muted;
  ctx.font = "600 14px 'Segoe UI', sans-serif";
  ctx.fillText(
    t("%d 圈 · 每圈 %@").replace("%d", String(lap.completedLaps + (lap.currentLapProgress > 0 ? 1 : 0))).replace("%@", formatTokens(goal, true)),
    cx,
    cy + 28
  );

  // 4) Comparison text (today vs yesterday / yesterday vs day-before).
  let yCmp = cy + r + 44;
  ctx.fillStyle = C.mutedStrong;
  ctx.font = "700 17px 'Segoe UI', sans-serif";
  ctx.textAlign = "center";
  const cmp = comparisonText(day, opts.previousDay);
  ctx.fillText(cmp, cx, yCmp);

  // 5) Breakdown panels: sources (tools) + models.
  const panelY = yCmp + 28;
  drawShareBreakdownPanel(ctx, C, pad, panelY, W - pad * 2, t("今日来源"), day.tools || {}, true);
  drawShareBreakdownPanel(
    ctx,
    C,
    pad,
    panelY + 150,
    W - pad * 2,
    t("主力模型"),
    day.models || {},
    false
  );

  // 6) 30-day trend panel (stacked mini bars).
  const trendY = panelY + 150 + 130;
  drawShareTrendPanel(ctx, C, pad, trendY, W - pad * 2, opts.daily30 || [], goal);

  // 7) Footer signature (preserved).
  ctx.fillStyle = C.mutedFaint;
  ctx.font = "600 13px 'Segoe UI', sans-serif";
  ctx.textAlign = "center";
  ctx.fillText("TokenStep · " + t("十七°") + " · " + t("本地统计"), W / 2, H - 22);

  return canvas;
}

// Three-state comparison line (port of `comparisonText`).
function comparisonText(day, previousDay) {
  if (!previousDay || !previousDay.total_tokens || previousDay.total_tokens <= 0) {
    return t("这是一个新的记录日");
  }
  const delta =
    ((day.total_tokens - previousDay.total_tokens) / previousDay.total_tokens) * 100;
  if (Math.abs(delta) < 1) return t("和前一天基本持平");
  if (delta > 0) return t("比前一天多 %@").replace("%@", formatPercent(Math.abs(delta)));
  return t("比前一天少 %@").replace("%@", formatPercent(Math.abs(delta)));
}

// A breakdown panel: title + up to 4 rows (color dot + name + tokens + bar).
function drawShareBreakdownPanel(ctx, C, x, y, w, title, values, useToolColor) {
  // Panel background.
  ctx.fillStyle = C.surface;
  ctx.strokeStyle = "rgba(0,0,0,0.06)";
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.roundRect(x, y, w, 120, 16);
  ctx.fill();
  ctx.stroke();
  // Title.
  ctx.fillStyle = C.muted;
  ctx.font = "700 13px 'Segoe UI', sans-serif";
  ctx.textAlign = "left";
  ctx.fillText(title, x + 16, y + 24);
  // Rows.
  const total = Math.max(dayValuesSum(values), 1);
  const entries = Object.keys(values)
    .filter(function (k) { return values[k] > 0; })
    .sort(function (a, b) { return values[b] - values[a]; })
    .slice(0, 4);
  const rowH = 20;
  const rowStart = y + 40;
  entries.forEach(function (name, i) {
    const tokens = values[name];
    const pct = (tokens / total) * 100;
    const ry = rowStart + i * rowH;
    const color = useToolColor ? tokenToolColor(name) : C.green;
    // Color dot.
    ctx.fillStyle = color;
    ctx.beginPath();
    ctx.arc(x + 22, ry - 4, 5, 0, Math.PI * 2);
    ctx.fill();
    // Name.
    ctx.fillStyle = C.ink;
    ctx.font = "600 13px 'Segoe UI', sans-serif";
    ctx.textAlign = "left";
    ctx.fillText(name, x + 34, ry);
    // Tokens (right-aligned).
    ctx.fillStyle = C.muted;
    ctx.textAlign = "right";
    ctx.fillText(formatTokens(tokens, true), x + w - 16, ry);
    // Progress bar (below the row text).
    const barX = x + 34;
    const barW = w - 50;
    ctx.fillStyle = C.track;
    ctx.beginPath();
    ctx.roundRect(barX, ry + 4, barW, 5, 2.5);
    ctx.fill();
    ctx.fillStyle = color;
    ctx.beginPath();
    ctx.roundRect(barX, ry + 4, (barW * pct) / 100, 5, 2.5);
    ctx.fill();
  });
  if (!entries.length) {
    ctx.fillStyle = C.mutedFaint;
    ctx.font = "600 13px 'Segoe UI', sans-serif";
    ctx.textAlign = "center";
    ctx.fillText(t("暂无数据"), x + w / 2, rowStart + 10);
  }
}

function dayValuesSum(values) {
  var s = 0;
  for (var k in values) s += Number(values[k] || 0);
  return s;
}

// 30-day trend: stacked mini bars (one per day, colored by tool).
function drawShareTrendPanel(ctx, C, x, y, w, daily, goal) {
  ctx.fillStyle = C.surface;
  ctx.strokeStyle = "rgba(0,0,0,0.06)";
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.roundRect(x, y, w, 116, 16);
  ctx.fill();
  ctx.stroke();
  ctx.fillStyle = C.muted;
  ctx.font = "700 13px 'Segoe UI', sans-serif";
  ctx.textAlign = "left";
  ctx.fillText(t("最近 30 天"), x + 16, y + 24);
  const rows = daily.slice(-30);
  if (!rows.length) return;
  const maxTokens = Math.max.apply(null, [goal].concat(rows.map(function (d) { return d.total_tokens; })).concat([1]));
  const barsY = y + 38;
  const barsH = 64;
  const barGap = 2;
  const barW = Math.max(3, (w - 32 - barGap * (rows.length - 1)) / rows.length);
  rows.forEach(function (d, i) {
    const bx = x + 16 + i * (barW + barGap);
    const totalH = Math.max(2, (d.total_tokens / maxTokens) * barsH);
    if (d.total_tokens <= 0) return;
    const segs = orderedToolEntries(d.tools || {});
    if (!segs.length) {
      ctx.fillStyle = contributionColor(d.total_tokens, goal);
      ctx.fillRect(bx, barsY + barsH - totalH, barW, totalH);
      return;
    }
    let drawn = 0;
    segs.slice().reverse().forEach(function (s) {
      const sh = Math.max(1, (totalH * s.tokens) / Math.max(d.total_tokens, 1));
      ctx.fillStyle = tokenToolColor(s.name);
      ctx.fillRect(bx, barsY + barsH - drawn - sh, barW, sh);
      drawn += sh;
    });
  });
}

// ---- Rhythm share card (port of ShareRhythmCardView) ----
// opts.rhythm — DailyRhythm {buckets[24], peak_hour, primary_tag, ...}
// opts.date   — "YYYY-MM-DD"
function renderRhythmCard(canvas, opts) {
  const ctx = canvas.getContext("2d");
  _ensureRoundRect(ctx);
  const W = canvas.width;
  const H = canvas.height;
  const rhythm = opts.rhythm || { buckets: [], primary_tag: "quiet_day" };
  const tag = rhythm.primary_tag || "quiet_day";
  const palette = rhythmPalette(tag);

  // 1) Dark neon background: radial gradient + faint grid.
  const bg = ctx.createRadialGradient(W / 2, H * 0.35, 40, W / 2, H * 0.5, W);
  bg.addColorStop(0, palette.bgInner);
  bg.addColorStop(1, palette.bgOuter);
  ctx.fillStyle = bg;
  ctx.fillRect(0, 0, W, H);
  // Grid.
  ctx.strokeStyle = "rgba(255,255,255,0.04)";
  ctx.lineWidth = 1;
  for (var gx = 0; gx < W; gx += 32) {
    ctx.beginPath();
    ctx.moveTo(gx, 0);
    ctx.lineTo(gx, H);
    ctx.stroke();
  }
  for (var gy = 0; gy < H; gy += 32) {
    ctx.beginPath();
    ctx.moveTo(0, gy);
    ctx.lineTo(W, gy);
    ctx.stroke();
  }

  const cx = W / 2;
  const pad = 40;

  // 2) Header: glowing brand + date + weekday.
  ctx.save();
  ctx.shadowColor = palette.glow;
  ctx.shadowBlur = 16;
  ctx.fillStyle = "#ffffff";
  ctx.font = "800 30px 'Segoe UI', sans-serif";
  ctx.textAlign = "center";
  ctx.fillText("TokenStep", cx, pad + 24);
  ctx.restore();
  ctx.fillStyle = "rgba(255,255,255,0.6)";
  ctx.font = "600 14px 'Segoe UI', sans-serif";
  ctx.fillText(formatDateWeekday(opts.date), cx, pad + 48);

  // 3) Rhythm tag (gradient text) + laurel branches.
  const tagY = pad + 96;
  drawLaurel(ctx, cx - 150, tagY - 6, -1, palette.accent);
  drawLaurel(ctx, cx + 150, tagY - 6, 1, palette.accent);
  ctx.save();
  const grad = ctx.createLinearGradient(cx - 120, tagY, cx + 120, tagY);
  grad.addColorStop(0, palette.accent);
  grad.addColorStop(1, palette.accent2);
  ctx.fillStyle = grad;
  ctx.font = "800 36px 'Segoe UI', sans-serif";
  ctx.textAlign = "center";
  ctx.fillText(rhythmTagTitle(tag), cx, tagY);
  ctx.restore();

  // 4) Neon wave: Catmull-Rom smooth curve + area fill + peak marker.
  const waveY = tagY + 60;
  const waveH = 200;
  drawRhythmWave(ctx, rhythm, cx, waveY, W - pad * 2, waveH, palette);

  // 5) Hour axis with day/moon icons.
  const axisY = waveY + waveH + 8;
  drawRhythmAxis(ctx, pad, axisY, W - pad * 2, palette);

  // 6) Token console: total tokens big number with chevrons.
  const tokenY = axisY + 50;
  ctx.fillStyle = "rgba(255,255,255,0.55)";
  ctx.font = "600 13px 'Segoe UI', sans-serif";
  ctx.textAlign = "center";
  ctx.fillText(t("Token 总量"), cx, tokenY);
  ctx.fillStyle = "#ffffff";
  ctx.font = "800 44px 'Segoe UI', sans-serif";
  ctx.fillText(formatTokens(rhythm.total_tokens || 0), cx, tokenY + 40);

  // 7) Three-metric footer: active span / night share / longest streak.
  const mY = tokenY + 80;
  const metrics = rhythmMetrics(rhythm);
  const mW = (W - pad * 2) / 3;
  metrics.forEach(function (m, i) {
    const mx = pad + i * mW;
    ctx.fillStyle = "rgba(255,255,255,0.5)";
    ctx.font = "600 12px 'Segoe UI', sans-serif";
    ctx.textAlign = "center";
    ctx.fillText(m.label, mx + mW / 2, mY);
    ctx.fillStyle = palette.accent;
    ctx.font = "800 20px 'Segoe UI', sans-serif";
    ctx.fillText(m.value, mx + mW / 2, mY + 24);
  });

  // 8) Footer signature.
  ctx.fillStyle = "rgba(255,255,255,0.35)";
  ctx.font = "600 12px 'Segoe UI', sans-serif";
  ctx.textAlign = "center";
  ctx.fillText("TokenStep · " + t("十七°") + " · " + t("本地统计"), cx, H - 20);

  return canvas;
}

// 4 palettes keyed by rhythm tag (port of RhythmCardPalette).
function rhythmPalette(tag) {
  switch (tag) {
    case "night_agent":
      return { bgInner: "#1a1033", bgOuter: "#0a0618", glow: "#a855f7", accent: "#c084fc", accent2: "#e879f9" };
    case "morning_planner":
    case "early_starter":
      return { bgInner: "#2a1d08", bgOuter: "#100a02", glow: "#f59e0b", accent: "#fbbf24", accent2: "#fde68a" };
    case "fragmented":
      return { bgInner: "#04212a", bgOuter: "#020e12", glow: "#06b6d4", accent: "#22d3ee", accent2: "#67e8f9" };
    default:
      return { bgInner: "#04230f", bgOuter: "#020c05", glow: "#22c55e", accent: "#4ade80", accent2: "#86efac" };
  }
}

// Draw a laurel branch (simplified) — `dir` = -1 left, 1 right.
function drawLaurel(ctx, x, y, dir, color) {
  ctx.save();
  ctx.translate(x, y);
  ctx.scale(dir, 1);
  ctx.strokeStyle = color;
  ctx.globalAlpha = 0.7;
  ctx.lineWidth = 2;
  ctx.beginPath();
  ctx.moveTo(0, 0);
  ctx.quadraticCurveTo(20, -14, 40, -4);
  ctx.stroke();
  // Leaves.
  for (var i = 0; i < 4; i++) {
    ctx.beginPath();
    ctx.ellipse(10 + i * 9, -8 - i * 1.5, 7, 3.5, -0.6, 0, Math.PI * 2);
    ctx.fillStyle = color;
    ctx.globalAlpha = 0.5 + i * 0.1;
    ctx.fill();
  }
  ctx.restore();
}

// Smooth Catmull-Rom wave with area fill, glow, and a peak marker dot.
function drawRhythmWave(ctx, rhythm, cx, y, w, h, palette) {
  const buckets = rhythm.buckets || [];
  if (!buckets.length) return;
  const max = Math.max.apply(null, buckets.map(function (b) { return b.tokens || 0; }).concat([1]));
  const stepX = w / 23;
  const points = buckets.map(function (b, i) {
    const tokens = b.tokens || 0;
    return { x: (cx - w / 2) + i * stepX, y: y + h - (tokens / max) * (h - 10), v: tokens };
  });
  // Area fill (under the curve).
  ctx.save();
  ctx.beginPath();
  ctx.moveTo(points[0].x, y + h);
  catmullRomPath(ctx, points, 0.5);
  ctx.lineTo(points[points.length - 1].x, y + h);
  ctx.closePath();
  const areaGrad = ctx.createLinearGradient(0, y, 0, y + h);
  areaGrad.addColorStop(0, palette.accent + "55");
  areaGrad.addColorStop(1, palette.accent + "00");
  ctx.fillStyle = areaGrad;
  ctx.fill();
  ctx.restore();
  // Glowing stroke line.
  ctx.save();
  ctx.shadowColor = palette.glow;
  ctx.shadowBlur = 14;
  ctx.strokeStyle = palette.accent;
  ctx.lineWidth = 3;
  ctx.lineJoin = "round";
  ctx.beginPath();
  ctx.moveTo(points[0].x, points[0].y);
  catmullRomPath(ctx, points, 0.5);
  ctx.stroke();
  ctx.restore();
  // Peak marker.
  if (rhythm.peak_hour != null) {
    const peakPt = points[rhythm.peak_hour];
    if (peakPt) {
      ctx.save();
      ctx.shadowColor = palette.glow;
      ctx.shadowBlur = 10;
      ctx.fillStyle = "#ffffff";
      ctx.beginPath();
      ctx.arc(peakPt.x, peakPt.y, 5, 0, Math.PI * 2);
      ctx.fill();
      ctx.restore();
    }
  }
}

// Append a Catmull-Rom spline through the points to the current path.
function catmullRomPath(ctx, pts, tension) {
  if (pts.length < 2) return;
  for (var i = 0; i < pts.length - 1; i++) {
    var p0 = pts[i - 1] || pts[i];
    var p1 = pts[i];
    var p2 = pts[i + 1];
    var p3 = pts[i + 2] || p2;
    var cp1x = p1.x + ((p2.x - p0.x) * tension) / 6;
    var cp1y = p1.y + ((p2.y - p0.y) * tension) / 6;
    var cp2x = p2.x - ((p3.x - p1.x) * tension) / 6;
    var cp2y = p2.y - ((p3.y - p1.y) * tension) / 6;
    ctx.bezierCurveTo(cp1x, cp1y, cp2x, cp2y, p2.x, p2.y);
  }
}

// Hour axis: 0/6/12/18/23 with day/moon icons at the ends.
function drawRhythmAxis(ctx, x, y, w, palette) {
  const marks = [0, 6, 12, 18, 23];
  ctx.fillStyle = "rgba(255,255,255,0.4)";
  ctx.font = "600 11px 'Segoe UI', sans-serif";
  ctx.textAlign = "center";
  marks.forEach(function (h) {
    const px = x + (h / 23) * w;
    ctx.fillText(h + ":00", px, y + 12);
  });
}

// Format a date as "6月22日 周一".
function formatDateWeekday(dateStr) {
  if (!dateStr) return "";
  var d = new Date(dateStr + "T00:00:00");
  if (isNaN(d.getTime())) return dateStr;
  var weekdays = [t("周日"), t("周一"), t("周二"), t("周三"), t("周四"), t("周五"), t("周六")];
  return (d.getMonth() + 1) + t("月") + d.getDate() + t("日") + " " + weekdays[d.getDay()];
}

// Three rhythm metrics (active span / night share / longest streak).
function rhythmMetrics(rhythm) {
  var buckets = rhythm.buckets || [];
  var first = rhythm.first_active_hour != null ? rhythm.first_active_hour : null;
  var last = rhythm.last_active_hour != null ? rhythm.last_active_hour : null;
  var span = first != null && last != null ? (first + "-" + last) : "—";
  // Night share: hours 21,22,23,0,1,2.
  var night = 0, total = 0, streak = 0, maxStreak = 0;
  for (var i = 0; i < buckets.length; i++) {
    var tk = buckets[i].tokens || 0;
    total += tk;
    if ((i >= 21 || i <= 2) && tk > 0) night += tk;
    if (tk > 0) { streak++; if (streak > maxStreak) maxStreak = streak; } else { streak = 0; }
  }
  var nightPct = total > 0 ? Math.round((night / total) * 100) : 0;
  return [
    { label: t("活跃时段"), value: span + ":00" },
    { label: t("夜间占比"), value: nightPct + "%" },
    { label: t("最长连续"), value: maxStreak + t("h") },
  ];
}

// ---- Screenshot / data export (port of ScreenshotExporter.swift) ----
// Download the current dashboard as an HTML self-contained file, or export the
// raw data as CSV/JSON.
// ---- Contribution wall (port of Components.swift ContributionWallView) ----
// GitHub-style activity heatmap: weeks as columns, days as rows.
function contributionWallHTML(rows, goal, weeks = 53) {
  const byDate = {};
  rows.forEach((d) => { byDate[d.date] = d; });
  const today = new Date();
  const start = new Date(today);
  start.setDate(start.getDate() - (weeks * 7 - 1));
  const mondayOffset = (start.getDay() + 6) % 7;
  start.setDate(start.getDate() - mondayOffset);
  let cols = "";
  for (let w = 0; w < weeks; w++) {
    let col = "";
    for (let d = 0; d < 7; d++) {
      const day = new Date(start);
      day.setDate(start.getDate() + w * 7 + d);
      if (day > today) { col += '<div class="wall-cell" style="background:transparent"></div>'; continue; }
      const key = day.getFullYear() + "-" + String(day.getMonth() + 1).padStart(2, "0") + "-" + String(day.getDate()).padStart(2, "0");
      const tokens = byDate[key] ? byDate[key].total_tokens : 0;
      const isToday = key === todayKey();
      const outline = isToday ? ";outline:2px solid var(--green);outline-offset:1px" : "";
      col += '<div class="wall-cell" style="background:' + contributionColor(tokens, goal) + outline + '" title="' + key + " " + formatTokens(tokens) + (isToday ? " (" + t("今天") + ")" : "") + '"></div>';
    }
    cols += '<div class="wall-col">' + col + "</div>";
  }
  const activeCount = rows.filter((d) => d.total_tokens > 0).length;
  const goalCount = rows.filter((d) => d.total_tokens >= goal).length;
  const maxTokens = rows.length ? Math.max(...rows.map((d) => d.total_tokens)) : 0;
  return (
    '<div class="wall">' + cols + "</div>" +
    '<div style="display:flex;gap:12px;flex-wrap:wrap;margin-top:14px">' +
    '<span class="pill"><span class="pill-label">' + t("活跃") + '</span><span class="pill-value">' + activeCount + t(" 天") + "</span></span>" +
    '<span class="pill"><span class="pill-label">' + t("达标") + '</span><span class="pill-value">' + goalCount + t(" 天") + "</span></span>" +
    '<span class="pill"><span class="pill-label">' + t("最高") + '</span><span class="pill-value">' + formatTokens(maxTokens, true) + "</span></span>" +
    "</div>" +
    '<div style="display:flex;align-items:center;gap:6px;margin-top:10px;font-size:12px;color:var(--muted);font-weight:600"><span>' + t("少") + '</span>' + ["#ebedf0", "#9be9a8", "#40c463", "#30a14e", "#216e39"].map(function (c) { return '<span style="width:12px;height:12px;border-radius:3px;background:' + c + '"></span>'; }).join("") + '<span>' + t("多") + '</span></div>'
  );
}

// ---- Tool color mapping (port of Components.swift tokenToolColor) ----
// Drives the stacked activity bars + legend so each client gets a stable hue.
function tokenToolColor(tool) {
  switch (tool) {
    case "Codex":
      return "#2da44e"; // green
    case "Claude Code":
      return "rgb(214,107,61)"; // orange-red (0.84,0.42,0.24) brightened
    case "Hermes":
    case "Hermes Agent":
      return "rgb(128,71,235)"; // violet
    default:
      // CC Switch variants → blue family so they're visually grouped.
      if (tool.indexOf("CC Switch") >= 0) {
        return tool.indexOf("Codex") >= 0
          ? "#0ea5e9" // ocean blue (Codex via CC Switch)
          : tool.indexOf("Gemini") >= 0
            ? "#8b5cf6" // violet (Gemini via CC Switch)
            : "#0891b2"; // teal (Claude Code / unknown via CC Switch)
      }
      return "rgba(31,41,55,0.44)"; // graphite fallback
  }
}

// Preferred display order for tools (port of orderedToolEntries).
function orderedToolEntries(tools) {
  // tools: { name: tokens } map (or array of DailyUsage-shaped rows for the
  // legend helper). Normalize to an array of {name, tokens} sorted by the
  // preferred list first, then by token count.
  const preferred = ["Codex", "Claude Code", "Hermes", "Hermes Agent"];
  const entries = [];
  for (const name of preferred) {
    const v = Number(tools[name] || 0);
    if (v > 0) entries.push({ name, tokens: v });
  }
  const rest = Object.keys(tools)
    .filter((k) => preferred.indexOf(k) < 0 && Number(tools[k]) > 0)
    .sort((a, b) => Number(tools[b]) - Number(tools[a]))
    .map((k) => ({ name: k, tokens: Number(tools[k]) }));
  return entries.concat(rest);
}

// ---- Stacked activity bars (port of StackedActivityBarsView) ----
// Each day is one vertical bar, split into colored segments per tool.
function stackedActivityBarsHTML(rows, goal) {
  if (!rows || !rows.length) return '<div class="empty">' + t("暂无活动数据") + "</div>";
  const maxTokens = Math.max.apply(
    null,
    [goal].concat(rows.map((d) => d.total_tokens)).concat([1])
  );
  return (
    '<div class="activity" style="margin-top:14px">' +
    rows
      .map((d) => {
        const totalHeight = Math.max(4, (d.total_tokens / maxTokens) * 100);
        if (d.total_tokens <= 0) {
          return '<div class="bar" style="height:4px;background:transparent"></div>';
        }
        const segments = orderedToolEntries(d.tools || {});
        if (!segments.length) {
          return (
            '<div class="bar" style="height:' +
            totalHeight +
            "%;background:" +
            contributionColor(d.total_tokens, goal) +
            '"></div>'
          );
        }
        // Stack segments bottom-up (preferred tool on top visually).
        const segHtml = segments
          .slice()
          .reverse()
          .map((s) => {
            const h = Math.max(1, (totalHeight * s.tokens) / Math.max(d.total_tokens, 1));
            return (
              '<div style="width:100%;height:' +
              h +
              "%;background:" +
              tokenToolColor(s.name) +
              '"></div>'
            );
          })
          .join("");
        return '<div class="bar" style="height:' + totalHeight + "%;" + '" title="' + d.date + " " + formatTokens(d.total_tokens) + '">' + segHtml + "</div>";
      })
      .join("") +
    "</div>"
  );
}

// Legend listing the tools present across the given daily rows.
function tokenToolLegendHTML(rows) {
  const seen = new Set();
  const names = [];
  for (const d of rows) {
    for (const e of orderedToolEntries(d.tools || {})) {
      if (!seen.has(e.name)) {
        seen.add(e.name);
        names.push(e.name);
        if (names.length >= 4) break;
      }
    }
    if (names.length >= 4) break;
  }
  if (!names.length) names.push("Codex", "Claude Code");
  return (
    '<div style="display:flex;gap:12px;flex-wrap:wrap;margin-top:14px">' +
    names
      .map(
        (n) =>
          '<span style="display:inline-flex;align-items:center;gap:5px;font-size:13px;color:var(--muted);font-weight:600">' +
          '<span style="width:9px;height:9px;border-radius:50%;background:' +
          tokenToolColor(n) +
          '"></span>' +
          n +
          "</span>"
      )
      .join("") +
    "</div>"
  );
}

// ---- Rhythm (24h hourly bars) ----
// Port of the macOS rhythm chart: 24 vertical bars (one per hour 0-23),
// peak hour highlighted. Reuses the .activity/.bar CSS used by the 30-day
// stacked chart (24 bars fit the flex:1 layout cleanly).
function hourlyBarsHTML(rhythm) {
  if (!rhythm || !rhythm.buckets || !rhythm.buckets.length)
    return '<div class="empty">' + t("暂无节奏数据") + "</div>";
  // Smooth wave version (port of macOS RhythmLineShape, SVG).
  return hourlyWaveHTML(rhythm);
}

// SVG smooth wave for the 24h rhythm chart (port of macOS RhythmLineShape).
// Renders a Catmull-Rom curve + gradient area fill + peak marker.
function hourlyWaveHTML(rhythm) {
  if (!rhythm || !rhythm.buckets || !rhythm.buckets.length)
    return '<div class="empty">' + t("暂无节奏数据") + "</div>";
  var buckets = rhythm.buckets;
  var max = Math.max.apply(null, buckets.map(function (b) { return b.tokens || 0; }).concat([1]));
  var W = 520, H = 100, pad = 6;
  var stepX = (W - pad * 2) / 23;
  var pts = buckets.map(function (b, i) {
    var tokens = b.tokens || 0;
    return { x: pad + i * stepX, y: H - pad - (tokens / max) * (H - pad * 2), v: tokens };
  });
  // Build the smooth path (Catmull-Rom → bezier segments).
  var linePath = "M" + pts[0].x + "," + pts[0].y;
  for (var i = 0; i < pts.length - 1; i++) {
    var p0 = pts[i - 1] || pts[i];
    var p1 = pts[i], p2 = pts[i + 1], p3 = pts[i + 2] || p2;
    var cp1x = p1.x + ((p2.x - p0.x) * 0.5) / 6;
    var cp1y = p1.y + ((p2.y - p0.y) * 0.5) / 6;
    var cp2x = p2.x - ((p3.x - p1.x) * 0.5) / 6;
    var cp2y = p2.y - ((p3.y - p1.y) * 0.5) / 6;
    linePath += " C" + cp1x + "," + cp1y + " " + cp2x + "," + cp2y + " " + p2.x + "," + p2.y;
  }
  var areaPath = linePath + " L" + pts[pts.length - 1].x + "," + (H - pad) + " L" + pts[0].x + "," + (H - pad) + " Z";
  var peakCircle = "";
  if (rhythm.peak_hour != null && pts[rhythm.peak_hour]) {
    var pp = pts[rhythm.peak_hour];
    peakCircle = '<circle cx="' + pp.x + '" cy="' + pp.y + '" r="4" fill="#fff" stroke="var(--green)" stroke-width="2"/>';
  }
  return '<svg viewBox="0 0 ' + W + ' ' + H + '" style="width:100%;height:100px;margin-top:14px">' +
    '<defs><linearGradient id="rhGrad" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="var(--green)" stop-opacity="0.35"/><stop offset="100%" stop-color="var(--green)" stop-opacity="0"/></linearGradient></defs>' +
    '<path d="' + areaPath + '" fill="url(#rhGrad)"/>' +
    '<path d="' + linePath + '" fill="none" stroke="var(--green)" stroke-width="2.5" stroke-linejoin="round" stroke-linecap="round"/>' +
    peakCircle +
    "</svg>";
}

// Hour labels under the 24h chart (0 / 6 / 12 / 18 / 23).
function hourlyAxisHTML() {
  var marks = [0, 6, 12, 18, 23];
  return (
    '<div style="display:flex;justify-content:space-between;margin-top:6px;font-size:11px;color:var(--muted);font-weight:600">' +
    marks.map(function (h) { return "<span>" + h + ":00</span>"; }).join("") +
    "</div>"
  );
}

// Map a RhythmTag (snake_case from the backend) to its localized title key.
// Keep in sync with i18n.js (each value here is an i18n key).
var RHYTHM_TAG_TITLE = {
  early_starter: "清晨启动型",
  morning_planner: "上午规划型",
  afternoon_burst: "下午爆发型",
  evening_sprint: "晚间冲刺型",
  night_agent: "夜间 Agent 型",
  double_peak: "双峰推进型",
  fragmented: "碎片推进型",
  one_shot: "一鼓作气型",
  steady_cruise: "稳步推进型",
  quiet_day: "安静的一天",
};

function rhythmTagTitle(tag) {
  var key = RHYTHM_TAG_TITLE[tag];
  return key ? t(key) : tag;
}

// ---- Quota window card (shared by Codex + Claude quota) ----
// Renders one 5h/7d utilization window. `prefix` disambiguates element ids.
function quotaWindowHTML(label, pct, resetsAt, prefix) {
  var color = pct >= 80 ? "#ef4444" : pct >= 50 ? "var(--green-dark)" : "var(--green)";
  var html = '<div><div style="font-size:14px;color:var(--muted);font-weight:600;margin-bottom:6px">' + label + "</div>";
  html += '<div style="font-size:32px;font-weight:800;color:' + color + '">' + Math.round(pct) + "%</div>";
  html += '<div style="height:8px;background:var(--track);border-radius:999px;margin-top:6px;overflow:hidden"><div style="height:100%;width:' + Math.min(100, Math.round(pct)) + "%;background:" + color + ';border-radius:999px"></div></div>';
  if (resetsAt) html += '<div style="font-size:12px;color:var(--muted);margin-top:4px">' + t("重置于 ") + resetsAt + "</div>";
  var _ = prefix;
  return html + "</div>";
}

function downloadFile(filename, content, mime) {
  const blob = new Blob([content], { type: mime || "text/plain;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function snapshotToCSV(snapshot) {
  const header = "date,Codex,Claude Code,total_tokens,estimated_cost\n";
  const rows = (snapshot.daily || []).map((d) =>
    [d.date, d.tools["Codex"] || 0, d.tools["Claude Code"] || 0, d.total_tokens, d.cost].join(",")
  );
  return header + rows.join("\n");
}

// ---- Theme system (port of Theme.swift) ----
// 5 palettes, each mapping to the CSS custom properties in app.css.
// Global cache of the active theme's RAW color values (hex/rgb, not var()).
// Populated by applyTheme(); used by ringSvg/contributionColor/Canvas where
// CSS var() can't be resolved (SVG stop-color, Canvas fillStyle, etc.).
const themeColors = {
  canvas: "#f6f8fa", surface: "#ffffff", ink: "#1f2937",
  muted: "#6b7280", mutedFaint: "#9ca3af", mutedStrong: "#4b5563",
  green: "#2da44e", greenDark: "#216e39", mint: "#9be9a8",
  track: "#ebedf0", mintSoft: "#ddf4df",
  activity1: "#9be9a8", activity2: "#40c463", activity3: "#30a14e", activity4: "#216e39",
  ring1: "rgb(64,196,99)", ring2: "rgb(48,161,78)",
  ring3: "rgb(33,110,57)", ring4: "rgb(14,68,41)",
};

const THEMES = {
  green: {
    title: "青绿", sub: "默认",
    vars: {
      "--canvas": "#f6f8fa", "--surface": "#ffffff", "--ink": "#1f2937",
      "--green": "#2da44e", "--green-dark": "#216e39", "--mint": "#9be9a8",
      "--track": "#ebedf0", "--mint-soft": "#ddf4df",
      "--activity1": "#9be9a8", "--activity2": "#40c463",
      "--activity3": "#30a14e", "--activity4": "#216e39",
      "--ring1": "rgb(64,196,99)", "--ring2": "rgb(48,161,78)",
      "--ring3": "rgb(33,110,57)", "--ring4": "rgb(14,68,41)",
    },
  },
  ocean: {
    title: "海蓝", sub: "清爽",
    vars: {
      "--canvas": "#f5fafd", "--surface": "#feffff", "--ink": "#1f2937",
      "--green": "#0ea5e9", "--green-dark": "#0369a1", "--mint": "#bae6fd",
      "--track": "#eaf0f5", "--mint-soft": "#e0f2fe",
      "--activity1": "#bae6fd", "--activity2": "#38bdf8",
      "--activity3": "#0ea5e9", "--activity4": "#0369a1",
      "--ring1": "rgb(56,189,248)", "--ring2": "rgb(14,165,233)",
      "--ring3": "rgb(2,132,199)", "--ring4": "rgb(7,89,133)",
    },
  },
  violet: {
    title: "紫藤", sub: "Agent",
    vars: {
      "--canvas": "#faf8ff", "--surface": "#fffeff", "--ink": "#1f2937",
      "--green": "#8b5cf6", "--green-dark": "#5b21b6", "--mint": "#ddd6fe",
      "--track": "#eeebf5", "--mint-soft": "#ede9fe",
      "--activity1": "#ddd6fe", "--activity2": "#a78bfa",
      "--activity3": "#8b5cf6", "--activity4": "#5b21b6",
      "--ring1": "rgb(167,139,250)", "--ring2": "rgb(139,92,246)",
      "--ring3": "rgb(109,40,217)", "--ring4": "rgb(76,29,149)",
    },
  },
  amber: {
    title: "琥珀", sub: "温暖",
    vars: {
      "--canvas": "#fffaf2", "--surface": "#fffffc", "--ink": "#1f2937",
      "--green": "#f59e0b", "--green-dark": "#b45309", "--mint": "#fde68a",
      "--track": "#f4efe7", "--mint-soft": "#fef3c7",
      "--activity1": "#fef3c7", "--activity2": "#fbbf24",
      "--activity3": "#f59e0b", "--activity4": "#b45309",
      "--ring1": "rgb(251,191,36)", "--ring2": "rgb(245,158,11)",
      "--ring3": "rgb(180,83,9)", "--ring4": "rgb(120,53,15)",
    },
  },
  graphite: {
    title: "石墨", sub: "专注",
    vars: {
      "--canvas": "#f7f7f7", "--surface": "#ffffff", "--ink": "#1f2937",
      "--green": "#52525b", "--green-dark": "#27272a", "--mint": "#d4d4d8",
      "--track": "#e8e8ec", "--mint-soft": "#e4e4e7",
      "--activity1": "#d4d4d8", "--activity2": "#71717a",
      "--activity3": "#52525b", "--activity4": "#27272a",
      "--ring1": "rgb(113,113,122)", "--ring2": "rgb(82,82,91)",
      "--ring3": "rgb(63,63,70)", "--ring4": "rgb(24,24,27)",
    },
  },
};

function applyTheme(name) {
  const theme = THEMES[name] || THEMES.green;
  const root = document.documentElement;
  const v = theme.vars;
  for (const [k, val] of Object.entries(v)) {
    root.style.setProperty(k, val);
  }
  // Sync the raw-color cache so SVG/Canvas/contributionColor pick up the theme.
  themeColors.canvas = v["--canvas"];
  themeColors.surface = v["--surface"];
  themeColors.ink = v["--ink"];
  themeColors.green = v["--green"];
  themeColors.greenDark = v["--green-dark"];
  themeColors.mint = v["--mint"];
  themeColors.track = v["--track"];
  themeColors.mintSoft = v["--mint-soft"];
  themeColors.activity1 = v["--activity1"];
  themeColors.activity2 = v["--activity2"];
  themeColors.activity3 = v["--activity3"];
  themeColors.activity4 = v["--activity4"];
  themeColors.ring1 = v["--ring1"];
  themeColors.ring2 = v["--ring2"];
  themeColors.ring3 = v["--ring3"];
  themeColors.ring4 = v["--ring4"];
}

// ---- Expose to the page ----
window.TS = {
  invoke,
  listen,
  t,
  tf,
  formatTokens,
  formatMoney,
  formatPercent,
  formatGeneratedTime,
  formatInterval,
  contributionColor,
  todayKey,
  ringSvg,
  ICONS,
  lapProgress,
  downloadFile,
  snapshotToCSV,
  renderDataCard,
  THEMES,
  applyTheme,
  contributionWallHTML,
  tokenToolColor,
  orderedToolEntries,
  stackedActivityBarsHTML,
  tokenToolLegendHTML,
  quotaWindowHTML,
  hourlyBarsHTML,
  hourlyAxisHTML,
  rhythmTagTitle,
  renderShareDailyCard,
  renderRhythmCard,
  rhythmMetrics,
  comparisonText,
};
