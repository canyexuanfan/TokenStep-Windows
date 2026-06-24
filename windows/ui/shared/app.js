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
    rawProgress: t / safeGoal, // total completion across laps (can exceed 1.0)
    color: lapColors[Math.min(currentLap, lapColors.length) - 1] || lapColors[lapColors.length - 1],
    lapTitle: tf("第 %d 圈", currentLap),
  };
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

// ── Share-card primitives (port of macOS SwiftUI components) ─────────────

// Brand logo mark, port of TokenStepVectorMark (Components.swift:83).
// Draws at the TOP-LEFT corner of (x,y) with the given size.
function drawTokenStepMark(ctx, x, y, size, opts) {
  opts = opts || {};
  const surface = opts.surface || themeColors.surface;
  const arcGreen = "#40c463";
  ctx.save();
  // Background rounded square.
  ctx.fillStyle = surface;
  ctx.strokeStyle = "rgba(0,0,0,0.05)";
  ctx.lineWidth = Math.max(0.8, size * 0.015);
  ctx.beginPath();
  ctx.roundRect(x, y, size, size, size * 0.28);
  ctx.fill();
  ctx.stroke();
  // Green arc (2 cubic segments), lineWidth size*0.074, round caps.
  ctx.strokeStyle = arcGreen;
  ctx.lineWidth = size * 0.074;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  ctx.beginPath();
  ctx.moveTo(x + size * 0.211, y + size * 0.645);
  ctx.bezierCurveTo(
    x + size * 0.215, y + size * 0.365,
    x + size * 0.475, y + size * 0.176,
    x + size * 0.683, y + size * 0.284
  );
  ctx.stroke();
  ctx.beginPath();
  ctx.moveTo(x + size * 0.746, y + size * 0.358);
  ctx.bezierCurveTo(
    x + size * 0.858, y + size * 0.475,
    x + size * 0.812, y + size * 0.690,
    x + size * 0.655, y + size * 0.804
  );
  ctx.stroke();
  // Filled dot at (0.707, 0.311).
  ctx.fillStyle = arcGreen;
  ctx.beginPath();
  ctx.arc(x + size * 0.707, y + size * 0.311, (size * 0.105) / 2, 0, Math.PI * 2);
  ctx.fill();
  // 4 ascending step blocks.
  const steps = [
    { x: 0.285, y: 0.625, w: 0.074, h: 0.076, c: "#9be9a8" },
    { x: 0.393, y: 0.533, w: 0.074, h: 0.168, c: "#40c463" },
    { x: 0.500, y: 0.445, w: 0.074, h: 0.256, c: "#30a14e" },
    { x: 0.607, y: 0.348, w: 0.074, h: 0.354, c: "#216e39" },
  ];
  steps.forEach(function (s) {
    ctx.fillStyle = s.c;
    ctx.beginPath();
    // stepBlock uses .position which centers, so center = (x+w/2, y+h/2).
    ctx.roundRect(
      x + size * (s.x),
      y + size * (s.y),
      size * s.w,
      size * s.h,
      size * 0.022
    );
    ctx.fill();
  });
  ctx.restore();
}

// White rounded card surface with stroke + shadow (port of ShareCardSurface).
// Draws the surface occupying (x,y,w,h). The caller renders content inside
// accounting for the given padding.
function drawShareCardSurface(ctx, x, y, w, h, padding, radius) {
  ctx.save();
  // Shadow: fill the white card with an offset blurred shadow so the card
  // appears to float above the backdrop (stronger than the macOS 0.045 to
  // compensate for Canvas lacking SwiftUI's soft ambient shadow).
  ctx.save();
  ctx.shadowColor = "rgba(0,0,0,0.09)";
  ctx.shadowBlur = 22;
  ctx.shadowOffsetY = 10;
  ctx.fillStyle = themeColors.surface;
  ctx.beginPath();
  ctx.roundRect(x, y, w, h, radius);
  ctx.fill();
  ctx.restore();
  // Stroke (no shadow on the stroke pass).
  ctx.strokeStyle = "rgba(0,0,0,0.055)";
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.roundRect(x, y, w, h, radius);
  ctx.stroke();
  ctx.restore();
}

// Build a daily share card (today OR yesterday mode).
//   opts.day         — DailyUsage {date, tools:{}, models:{}, total_tokens, cost}
//   opts.previousDay — DailyUsage (for the "比前一天多/少 X%" comparison) or null
//   opts.daily30     — last 30 DailyUsage rows (trend panel + month avg)
//   opts.settings    — {daily_goal_tokens}
//   opts.mode        — "today" | "yesterday"
//   opts.totals      — {tokens, active_days} (for the metric strip) or null
//   opts.goalDays    — number of days that reached the goal (or null)
// Layout port of ShareDailyCardView.swift. Outer VStack(alignment:.leading,
// spacing:14) padded 28, on a TokenStepBackdrop, width 600, height自适应.
// Mac original is 600×840 but only fits its own blocks; this port adds the
// front-end-only lap-chips / pills / metric-strip, so the canvas grows taller
// to keep every block at its true macOS metrics (no squashing).
function renderShareDailyCard(canvas, opts) {
  const ctx = canvas.getContext("2d");
  _ensureRoundRect(ctx);
  const C = themeColors;
  const day = opts.day || { total_tokens: 0, cost: 0, tools: {}, models: {} };
  const settings = opts.settings || { daily_goal_tokens: 100000000 };
  const goal = Math.max(settings.daily_goal_tokens, 1);
  const lap = lapProgress(day.total_tokens, goal);
  const lapColor = lap.color || C.green;
  const pad = 28;
  const W = canvas.width || 600;
  const contentW = W - pad * 2;
  const isYesterday = opts.mode === "yesterday";

  // ── Pre-compute derived values used by both measure & draw passes. ──
  const daily30 = opts.daily30 || [];
  const monthAvg = daily30.length
    ? Math.round(daily30.reduce(function (a, d) { return a + (d.total_tokens || 0); }, 0) / 30)
    : 0;
  const totals = opts.totals || {};
  const goalDays = opts.goalDays != null ? opts.goalDays : 0;

  // lap-chip list (port of viewToday lapChips logic).
  const chipList = [];
  if (lap.completedLaps > 0) {
    var visible = lap.completedLaps <= 2 ? [] : [lap.completedLaps - 1];
    visible.forEach(function (n) {
      chipList.push({
        active: false,
        title: "✓ " + tf(t("%d圈完成"), n),
        detail: formatTokens(n * goal, true),
      });
    });
    chipList.push({
      active: true,
      title: "↻ " + tf(t("%@进行中"), lap.lapTitle),
      detail: formatPercent(lap.currentLapPercent),
    });
  }

  // ── Two-pass: first measure total height, then set canvas.height & draw. ──
  // Precise block heights (kept in sync with the draw pass below).
  const headerH = 42; // mark height
  const ringSize = 212;
  const ringBox = ringSize; // 212
  const pillsRowH = 40;
  const chipsGap = chipList.length ? 12 : 0;
  const chipsRowH = chipList.length ? 40 : 0;
  const heroContentH = ringBox + 18 + pillsRowH + chipsGap + chipsRowH;
  const heroH = 20 * 2 + heroContentH; // surface padding 20
  const metricStripH = 96; // one mini card height
  const footerH = 20;

  // Count breakdown rows to size those panels.
  const toolEntries = Object.keys(day.tools || {})
    .filter(function (k) { return day.tools[k] > 0; })
    .sort(function (a, b) { return day.tools[b] - day.tools[a]; })
    .slice(0, 4);
  const modelEntries = Object.keys(day.models || {})
    .filter(function (k) { return day.models[k] > 0; })
    .sort(function (a, b) { return day.models[b] - day.models[a]; })
    .slice(0, 4);
  // Breakdown panel height MUST equal drawShareBreakdownPanel's returned
  // panelH = padding*2(32) + headerH(34) + entries.length*rowH(25). Keep in
  // sync with that function. Empty data → entries.length 0 → 66px panel.
  const breakdownToolH = 16 * 2 + 34 + Math.max(toolEntries.length, 1) * 25;
  const breakdownModelH = 16 * 2 + 34 + Math.max(modelEntries.length, 1) * 25;
  // Trend panel height MUST equal drawShareTrendPanel's panelH (162).
  const trendH = 162;

  const blocks = [headerH, heroH, metricStripH, breakdownToolH, breakdownModelH, trendH];
  const spacing = 14;
  const totalH = pad + blocks.reduce(function (a, b) { return a + b; }, 0) + spacing * (blocks.length - 1) + 14 /* gap before footer */ + footerH + pad;

  // Apply measured height (idempotent: if caller already set a height, we
  // override it because content now drives the size).
  canvas.width = W;
  canvas.height = totalH;
  const H = canvas.height;
  _ensureRoundRect(ctx); // re-apply after resize (resize resets context state)

  // ── Backdrop: TokenStepBackdrop port — canvas color + faint diagonal tint. ──
  ctx.fillStyle = C.canvas;
  ctx.fillRect(0, 0, W, H);
  const tint = ctx.createLinearGradient(0, 0, W, H);
  tint.addColorStop(0, hexA(C.mint, 0.10));
  tint.addColorStop(0.5, "rgba(0,0,0,0)");
  tint.addColorStop(1, hexA(C.green, 0.025));
  ctx.fillStyle = tint;
  ctx.fillRect(0, 0, W, H);

  // ── Header: mark(42) + brand (left) | mode title + date (right). ──
  drawTokenStepMark(ctx, pad, pad, 42);
  ctx.textAlign = "left";
  ctx.fillStyle = C.ink;
  ctx.font = "800 27px 'Segoe UI', sans-serif";
  ctx.fillText("TokenStep", pad + 42 + 12, pad + 22);
  ctx.fillStyle = C.muted;
  ctx.font = "700 13px 'Segoe UI', sans-serif";
  ctx.fillText(t("每日 Token 消耗追踪"), pad + 42 + 12, pad + 38);
  // Right column: Mac shows mode title (headline heavy) over date (caption).
  ctx.textAlign = "right";
  ctx.fillStyle = C.ink;
  ctx.font = "800 18px 'Segoe UI', sans-serif";
  ctx.fillText(isYesterday ? t("昨日 AI 工作成绩单") : t("今日 AI 战绩"), W - pad, pad + 20);
  ctx.fillStyle = C.muted;
  ctx.font = "700 13px 'Segoe UI', sans-serif";
  ctx.fillText((day.date || "") + " " + formatRhythmWeekday(day.date), W - pad, pad + 38);

  let y = pad + headerH + 14;

  // ── Hero: ShareCardSurface(r26,pad20) holding three rows: ────────────
  //   row 1: ring (left) + right text block, vertically centered
  //   row 2: pills (2 equal columns, full width)
  //   row 3: lap-chips (full width, when present)
  drawShareCardSurface(ctx, pad, y, contentW, heroH, 20, 26);
  const heroInnerX = pad + 20;
  const heroInnerY = y + 20;
  const heroInnerW = contentW - 40;

  // Row 1 — ring (left) + right text block. Both vertically centered on the
  // ring's vertical center, matching macOS HStack(alignment:.center).
  const ringCx = heroInnerX + ringBox / 2;
  const ringCy = heroInnerY + ringBox / 2;
  // Soft glow behind ring — macOS uses Circle().fill(lap.color.opacity(0.09)).blur(10) at 228.
  ctx.save();
  ctx.fillStyle = hexA(lapColor, 0.09);
  ctx.filter = "blur(10px)";
  ctx.beginPath();
  ctx.arc(ringCx, ringCy, 114, 0, Math.PI * 2);
  ctx.fill();
  ctx.filter = "none";
  ctx.restore();
  // Track ring (full circle, lineCap round — matches ProgressRingView).
  const ringStroke = 20; // macOS share card passes lineWidth:20
  const ringR = (ringSize - ringStroke) / 2; // inner radius
  ctx.strokeStyle = C.track;
  ctx.lineWidth = ringStroke;
  ctx.lineCap = "round";
  ctx.beginPath();
  ctx.arc(ringCx, ringCy, ringR, 0, Math.PI * 2);
  ctx.stroke();
  // Progress ring: 12 o'clock start, clockwise, solid lapColor, light shadow.
  ctx.save();
  ctx.shadowColor = hexA(lapColor, 0.10);
  ctx.shadowBlur = 5;
  ctx.shadowOffsetY = 3;
  ctx.strokeStyle = lapColor;
  ctx.lineWidth = ringStroke;
  ctx.lineCap = "round";
  ctx.beginPath();
  ctx.arc(ringCx, ringCy, ringR, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * Math.max(0, Math.min(lap.currentLapProgress, 1)));
  ctx.stroke();
  ctx.restore();
  // Ring center: big token number (54px black) + per-lap goal label.
  // 54px (not 64) so the number fits the ring inner diameter comfortably.
  ctx.fillStyle = C.ink;
  ctx.textAlign = "center";
  ctx.font = "800 54px 'Segoe UI', sans-serif";
  ctx.fillText(formatTokens(day.total_tokens), ringCx, ringCy + 6);
  ctx.fillStyle = C.muted;
  ctx.font = "700 14px 'Segoe UI', sans-serif";
  ctx.fillText(tf(t("/ %@ 每圈"), formatTokens(goal, true)), ringCx, ringCy + 30);

  // Right text block — vertically centered against the ring center.
  const rightX = heroInnerX + ringBox + 24;
  const rightW = heroInnerW - ringBox - 24;
  // Compute total right-block height, then top = ringCy - height/2 (centered).
  const introH = 20, pctH = 58, infoLineH = 20;
  const infoGap = 4;
  const rightBlockH = introH + 6 + pctH + 10 + infoLineH * 3 + infoGap * 2;
  let ry = ringCy - rightBlockH / 2;
  ctx.textAlign = "left";
  // Intro line (callout heavy ~16px).
  ctx.fillStyle = C.muted;
  ctx.font = "700 16px 'Segoe UI', sans-serif";
  ctx.fillText(isYesterday ? t("昨天我和 AI 一起完成了") : t("今天我和 AI 一起消耗了"), rightX, ry + introH);
  ry += introH + 6;
  // Big completion% (58px black) in lap color, baseline-aligned label.
  const completionPct = Math.min(999, Math.round(lap.rawProgress * 100));
  const pctText = formatPercent(completionPct);
  ctx.fillStyle = lapColor;
  ctx.font = "900 52px 'Segoe UI', sans-serif";
  ctx.fillText(pctText, rightX, ry + pctH);
  const pctW = ctx.measureText(pctText).width;
  ctx.fillStyle = C.muted;
  ctx.font = "700 15px 'Segoe UI', sans-serif";
  // "总完成度" sits on the same baseline as the big number (alphabetic baseline).
  ctx.fillText(t("总完成度"), rightX + pctW + 8, ry + pctH - 8);
  ry += pctH + 10;
  // Three info lines, stacked by line height (no absolute offsets).
  // Line 1: completed laps (title3 heavy ~18px).
  ctx.fillStyle = hexA(C.ink, 0.82);
  ctx.font = "800 18px 'Segoe UI', sans-serif";
  const lapsTotal = lap.completedLaps + (lap.currentLapProgress > 0 ? 1 : 0);
  ctx.fillText(tf(t("已完成 %d 圈"), lapsTotal), rightX, ry + infoLineH);
  ry += infoLineH + infoGap;
  // Line 2: per-lap goal (subheadline bold ~14px).
  ctx.fillStyle = C.muted;
  ctx.font = "700 14px 'Segoe UI', sans-serif";
  ctx.fillText(tf(t("每圈目标 %@"), formatTokens(goal, true)), rightX, ry + infoLineH);
  ry += infoLineH + infoGap;
  // Line 3: comparison / 今日 Token (subheadline heavy ~14px).
  ctx.fillStyle = C.muted;
  ctx.font = "700 14px 'Segoe UI', sans-serif";
  const cmpLine = isYesterday ? comparisonText(day, opts.previousDay) : t("今日 Token");
  ctx.fillText(truncate(ctx, cmpLine, rightW), rightX, ry + infoLineH);

  // Row 2 — pills: two equal columns spanning full hero inner width.
  const pillsY = heroInnerY + ringBox + 18;
  const pillGap = 12;
  const pillW = (heroInnerW - pillGap) / 2;
  drawPill(ctx, heroInnerX, pillsY, pillW, pillsRowH, t("消耗金额"), formatMoney(day.cost));
  drawPill(ctx, heroInnerX + pillW + pillGap, pillsY, pillW, pillsRowH, t("本月均值"), formatTokens(monthAvg, true));

  // Row 3 — lap-chips: full width, only when present.
  if (chipList.length) {
    const chipsY = pillsY + pillsRowH + 12;
    drawLapChips(ctx, heroInnerX, chipsY, heroInnerW, chipsRowH, chipList, lapColor);
  }

  y += heroH + 14;

  // ── Metric strip: 累计 / 活跃 / 达标 (3 mini cards). ────────────────
  drawMetricStrip(ctx, pad, y, contentW, metricStripH, [
    { label: t("累计 Token 消耗"), value: formatTokens(totals.tokens || 0, true), detail: t("所有本机记录") },
    { label: t("活跃天数"), value: (totals.active_days || 0) + " " + t("天"), detail: t("有 AI 使用的日期") },
    { label: t("达标天数"), value: goalDays + " " + t("天"), detail: t("达到每日目标") },
  ]);
  y += metricStripH + 14;

  // ── Breakdown: 今日/昨日来源 (tools). ────────────────────────────────
  const b1H = drawShareBreakdownPanel(
    ctx, C, pad, y, contentW,
    isYesterday ? t("昨日来源") : t("今日来源"),
    t("颜色代表客户端"),
    day.tools || {}, true
  );
  y += b1H + 14;

  // ── Breakdown: 主力模型. ─────────────────────────────────────────────
  const b2H = drawShareBreakdownPanel(
    ctx, C, pad, y, contentW,
    t("主力模型"),
    t("按 Token 消耗排序"),
    day.models || {}, false
  );
  y += b2H + 14;

  // ── Trend panel (30-day bars + summary capsule). ─────────────────────
  drawShareTrendPanel(ctx, C, pad, y, contentW, daily30, goal, day);
  y += trendH + 14;

  // ── Footer: shield + 本地统计·不上传对话 (left) | tokenstep.app (right). ──
  const footerY = y + footerH - 6;
  ctx.textAlign = "left";
  ctx.fillStyle = C.muted;
  ctx.font = "700 12px 'Segoe UI', sans-serif";
  drawShield(ctx, pad, footerY - 9, C.muted);
  ctx.fillText(t("本地统计") + " · " + t("不上传代码或对话"), pad + 20, footerY);
  ctx.textAlign = "right";
  ctx.fillText("tokenstep.app", W - pad, footerY);

  return canvas;
}

// Render a "today overview" image for the full-page screenshot feature.
// This is the front-end-style counterpart to the macOS-style share card:
// a taller, wider canvas laying out hero + metric strip + 30-day activity +
// today's client/model distribution, mirroring the on-screen Today page.
//   opts.snapshot — full snapshot {totals, daily, tools, models, rhythms}
//   opts.settings — {daily_goal_tokens, ...}
// Width is fixed by the caller (720); height auto-grows to fit content.
function renderTodayOverview(canvas, opts) {
  const ctx = canvas.getContext("2d");
  _ensureRoundRect(ctx);
  const C = themeColors;
  const snap = opts.snapshot || {};
  const settings = opts.settings || { daily_goal_tokens: 100000000 };
  const goal = Math.max(settings.daily_goal_tokens, 1);
  const daily = snap.daily || [];
  const today = daily[daily.length - 1] || { total_tokens: 0, cost: 0, tools: {}, models: {} };
  const lap = lapProgress(today.total_tokens, goal);
  const lapColor = lap.color || C.green;

  const W = canvas.width || 720;
  const pad = 32;
  const contentW = W - pad * 2;
  const headerH = 48;

  // ── Measure each block so canvas.height fits exactly. ──
  const ringSize = 204;
  const ringBox = ringSize;
  const pillsRowH = 44;
  const pillGap = 14;
  const metricStripH = 104;
  const activityPanelH = 16 * 2 + 30 + 10 + 96 + 14 + 22; // pad + title + gap + bars + gap + legend
  // Distribution: two side-by-side panels. Height = pad*2 + title(28) + 5 rows*32.
  const distPanelH = 16 * 2 + 28 + 5 * 32;
  const footerH = 22;

  // lap-chip row (same rule as the share card).
  const chipList = [];
  if (lap.completedLaps > 0) {
    var visible = lap.completedLaps <= 2 ? [] : [lap.completedLaps - 1];
    visible.forEach(function (n) {
      chipList.push({ active: false, title: "✓ " + tf(t("%d圈完成"), n), detail: formatTokens(n * goal, true) });
    });
    chipList.push({ active: true, title: "↻ " + tf(t("%@进行中"), lap.lapTitle), detail: formatPercent(lap.currentLapPercent) });
  }
  const chipsRowH = chipList.length ? 44 : 0;
  const chipsGap = chipList.length ? 14 : 0;

  // Hero content height: ring row (ring tall) + chips + pills.
  const heroContentH = ringBox + chipsGap + chipsRowH + 16 + pillsRowH;
  const heroH = 22 * 2 + heroContentH; // surface padding 22

  const spacing = 16;
  const blocks = [headerH, heroH, metricStripH, activityPanelH, distPanelH];
  const totalH = pad + blocks.reduce(function (a, b) { return a + b; }, 0) + spacing * (blocks.length - 1) + 14 + footerH + pad;

  canvas.width = W;
  canvas.height = totalH;
  const H = canvas.height;
  _ensureRoundRect(ctx);

  // ── Backdrop. ──
  ctx.fillStyle = C.canvas;
  ctx.fillRect(0, 0, W, H);
  const tint = ctx.createLinearGradient(0, 0, W, H);
  tint.addColorStop(0, hexA(C.mint, 0.10));
  tint.addColorStop(0.5, "rgba(0,0,0,0)");
  tint.addColorStop(1, hexA(C.green, 0.025));
  ctx.fillStyle = tint;
  ctx.fillRect(0, 0, W, H);

  // ── Header. ──
  drawTokenStepMark(ctx, pad, pad, 48);
  ctx.textAlign = "left";
  ctx.fillStyle = C.ink;
  ctx.font = "800 30px 'Segoe UI', sans-serif";
  ctx.fillText("TokenStep", pad + 48 + 14, pad + 24);
  ctx.fillStyle = C.muted;
  ctx.font = "700 14px 'Segoe UI', sans-serif";
  ctx.fillText(t("每日 Token 消耗追踪"), pad + 48 + 14, pad + 42);
  ctx.textAlign = "right";
  ctx.fillStyle = C.ink;
  ctx.font = "800 20px 'Segoe UI', sans-serif";
  ctx.fillText(t("今日概览"), W - pad, pad + 24);
  ctx.fillStyle = C.muted;
  ctx.font = "700 13px 'Segoe UI', sans-serif";
  ctx.fillText((today.date || "") + " " + formatRhythmWeekday(today.date), W - pad, pad + 42);

  let y = pad + headerH + spacing;

  // ── Hero: ring (left) + right text block, then chips, then pills. ──
  drawShareCardSurface(ctx, pad, y, contentW, heroH, 22, 24);
  const hx = pad + 22;
  const hy = y + 22;
  const hw = contentW - 44;
  const ringCx = hx + ringBox / 2;
  const ringCy = hy + ringBox / 2;
  // Glow.
  ctx.save();
  ctx.fillStyle = hexA(lapColor, 0.09);
  ctx.filter = "blur(10px)";
  ctx.beginPath();
  ctx.arc(ringCx, ringCy, ringSize / 2 + 8, 0, Math.PI * 2);
  ctx.fill();
  ctx.filter = "none";
  ctx.restore();
  // Track + progress ring (stroke 20, like the front-end ringSvg).
  const ringStroke = 20;
  const ringR = (ringSize - ringStroke) / 2;
  ctx.strokeStyle = C.track;
  ctx.lineWidth = ringStroke;
  ctx.lineCap = "round";
  ctx.beginPath();
  ctx.arc(ringCx, ringCy, ringR, 0, Math.PI * 2);
  ctx.stroke();
  ctx.save();
  ctx.shadowColor = hexA(lapColor, 0.10);
  ctx.shadowBlur = 5;
  ctx.shadowOffsetY = 3;
  // Front-end ring uses a mint→green→greenDark diagonal gradient.
  const rg = ctx.createLinearGradient(ringCx - ringR, ringCy + ringR, ringCx + ringR, ringCy - ringR);
  rg.addColorStop(0, C.mint);
  rg.addColorStop(0.5, C.green);
  rg.addColorStop(1, C.greenDark);
  ctx.strokeStyle = rg;
  ctx.lineWidth = ringStroke;
  ctx.lineCap = "round";
  ctx.beginPath();
  ctx.arc(ringCx, ringCy, ringR, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * Math.max(0, Math.min(lap.currentLapProgress, 1)));
  ctx.stroke();
  ctx.restore();
  // Ring center: token number + per-lap goal (front-end sizing: 42px value).
  ctx.fillStyle = C.ink;
  ctx.textAlign = "center";
  ctx.font = "800 42px 'Segoe UI', sans-serif";
  ctx.fillText(formatTokens(today.total_tokens), ringCx, ringCy + 4);
  ctx.fillStyle = C.muted;
  ctx.font = "700 16px 'Segoe UI', sans-serif";
  ctx.fillText("/ " + formatTokens(goal, true) + " " + t("每圈"), ringCx, ringCy + 28);

  // Right text block — vertically centered on ring center (front-end layout).
  const rightX = hx + ringBox + 34;
  const rightW = hw - ringBox - 34;
  const lapTitleH = 40, subH = 26;
  const rightBlockH = lapTitleH + subH * 2 + 8;
  let ry = ringCy - rightBlockH / 2;
  ctx.textAlign = "left";
  // Lap title + percent (big, lap color) — front-end uses 35px.
  ctx.fillStyle = lapColor;
  ctx.font = "800 34px 'Segoe UI', sans-serif";
  ctx.fillText(lap.lapTitle + " · " + formatPercent(lap.currentLapPercent), rightX, ry + lapTitleH);
  ry += lapTitleH + 4;
  // 已完成 X.
  ctx.fillStyle = C.muted;
  ctx.font = "700 18px 'Segoe UI', sans-serif";
  ctx.fillText(t("已完成 ") + formatTokens(lap.completedLaps * goal, true), rightX, ry + subH);
  ry += subH + 4;
  // 每圈目标.
  ctx.fillStyle = C.muted;
  ctx.font = "700 18px 'Segoe UI', sans-serif";
  ctx.fillText(t("每圈目标 ") + formatTokens(goal, true), rightX, ry + subH);

  // Chips row (full width).
  let belowY = hy + ringBox;
  if (chipList.length) {
    belowY += chipsGap;
    drawLapChips(ctx, hx, belowY, hw, chipsRowH, chipList, lapColor);
    belowY += chipsRowH;
  }
  // Pills row: two equal columns.
  belowY += 16;
  const pillW = (hw - pillGap) / 2;
  const monthAvg = daily.slice(-30).length
    ? Math.round(daily.slice(-30).reduce(function (a, d) { return a + (d.total_tokens || 0); }, 0) / 30)
    : 0;
  drawPill(ctx, hx, belowY, pillW, pillsRowH, t("消耗金额"), formatMoney(today.cost));
  drawPill(ctx, hx + pillW + pillGap, belowY, pillW, pillsRowH, t("本月均值"), formatTokens(monthAvg, true));

  y += heroH + spacing;

  // ── Metric strip. ──
  const goalDays = daily.filter(function (d) { return d.total_tokens >= goal; }).length;
  drawMetricStrip(ctx, pad, y, contentW, metricStripH, [
    { label: t("累计 Token 消耗"), value: formatTokens((snap.totals || {}).tokens || 0, true), detail: t("所有本机记录") },
    { label: t("活跃天数"), value: ((snap.totals || {}).active_days || 0) + " " + t("天"), detail: t("有 AI 使用的日期") },
    { label: t("达标天数"), value: goalDays + " " + t("天"), detail: t("达到每日目标") },
  ]);
  y += metricStripH + spacing;

  // ── 30-day activity panel. ──
  drawOverviewActivityPanel(ctx, C, pad, y, contentW, daily.slice(-30), goal, today);
  y += activityPanelH + spacing;

  // ── Distribution: today's client + model (two side-by-side panels). ──
  const distGap = 16;
  const distW = (contentW - distGap) / 2;
  drawOverviewDistPanel(ctx, C, pad, y, distW, distPanelH, t("今日客户端"), today.tools || {}, true);
  drawOverviewDistPanel(ctx, C, pad + distW + distGap, y, distW, distPanelH, t("今日模型"), today.models || {}, false);
  y += distPanelH + 14;

  // ── Footer. ──
  const footerY = y + footerH - 4;
  ctx.textAlign = "left";
  ctx.fillStyle = C.muted;
  ctx.font = "700 12px 'Segoe UI', sans-serif";
  drawShield(ctx, pad, footerY - 9, C.muted);
  ctx.fillText(t("本地统计") + " · " + t("不上传代码或对话"), pad + 20, footerY);
  ctx.textAlign = "right";
  ctx.fillText("tokenstep.app", W - pad, footerY);

  return canvas;
}

// 30-day activity panel for the overview image. Taller bars (96) + a legend
// row beneath, mirroring the on-screen Today page's activity card.
function drawOverviewActivityPanel(ctx, C, x, y, w, daily, goal, today) {
  const padding = 16;
  drawShareCardSurface(ctx, x, y, w, 16 * 2 + 30 + 10 + 96 + 14 + 22, padding, 22);
  const innerX = x + padding;
  const innerW = w - padding * 2;
  // Title + subtitle.
  ctx.fillStyle = C.ink;
  ctx.font = "800 20px 'Segoe UI', sans-serif";
  ctx.textAlign = "left";
  ctx.fillText(t("最近 30 天"), innerX, y + padding + 18);
  ctx.fillStyle = C.muted;
  ctx.font = "600 14px 'Segoe UI', sans-serif";
  ctx.fillText(t("柱越高，用量越多；颜色代表客户端"), innerX, y + padding + 36);
  // Today capsule (right).
  const capText = t("今天") + " " + formatTokens((today && today.total_tokens) || 0, true);
  ctx.font = "700 14px 'Segoe UI', sans-serif";
  const capW = ctx.measureText(capText).width + 24;
  const capH = 28;
  const capX = x + w - padding - capW;
  const capY = y + padding + 6;
  ctx.fillStyle = hexA(C.mint, 0.28);
  ctx.beginPath();
  ctx.roundRect(capX, capY, capW, capH, capH / 2);
  ctx.fill();
  ctx.fillStyle = C.greenDark;
  ctx.textAlign = "center";
  ctx.fillText(capText, capX + capW / 2, capY + capH - 8);

  // Bars.
  const rows = daily.slice(-30);
  if (rows.length) {
    const maxTokens = Math.max.apply(null, [goal].concat(rows.map(function (d) { return d.total_tokens; })).concat([1]));
    const barsY = y + padding + 46;
    const barsH = 96;
    const barGap = 5;
    const barW = Math.max(4, (innerW - barGap * (rows.length - 1)) / rows.length);
    const barR = Math.min(4, barW / 2);
    rows.forEach(function (d, i) {
      const bx = innerX + i * (barW + barGap);
      const totalH = Math.max(4, (d.total_tokens / maxTokens) * barsH);
      if (d.total_tokens <= 0) {
        ctx.fillStyle = C.track;
        ctx.beginPath();
        ctx.roundRect(bx, barsY + barsH - 4, barW, 4, Math.min(2, barW / 2));
        ctx.fill();
        return;
      }
      const segs = orderedToolEntries(d.tools || {});
      if (!segs.length) {
        ctx.fillStyle = contributionColor(d.total_tokens, goal);
        ctx.beginPath();
        ctx.roundRect(bx, barsY + barsH - totalH, barW, totalH, barR);
        ctx.fill();
        return;
      }
      ctx.save();
      ctx.beginPath();
      ctx.roundRect(bx, barsY + barsH - totalH, barW, totalH, barR);
      ctx.clip();
      let drawn = 0;
      segs.slice().reverse().forEach(function (s) {
        const sh = Math.max(1, (totalH * s.tokens) / Math.max(d.total_tokens, 1));
        ctx.fillStyle = tokenToolColor(s.name);
        ctx.fillRect(bx, barsY + barsH - drawn - sh, barW, sh);
        drawn += sh;
      });
      ctx.restore();
    });
    // Goal reference line at goal/maxTokens (port of macOS 1pt .quaternary line).
    const goalLineY = barsY + barsH - (goal / maxTokens) * barsH;
    ctx.strokeStyle = hexA(C.muted, 0.45);
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(innerX, goalLineY);
    ctx.lineTo(innerX + innerW, goalLineY);
    ctx.stroke();
  }
  // Legend (top tools across the 30 days).
  const legendY = y + 16 * 2 + 30 + 10 + 96 + 14;
  drawOverviewLegend(ctx, C, innerX, legendY, innerW, rows);
}

// Compact legend for the activity bars: top tools by total tokens.
function drawOverviewLegend(ctx, C, x, y, maxW, rows) {
  const totals = {};
  rows.forEach(function (d) {
    Object.keys(d.tools || {}).forEach(function (k) {
      totals[k] = (totals[k] || 0) + (d.tools[k] || 0);
    });
  });
  const entries = Object.keys(totals).sort(function (a, b) { return totals[b] - totals[a]; }).slice(0, 4);
  if (!entries.length) return;
  ctx.textAlign = "left";
  ctx.textBaseline = "middle";
  let lx = x;
  entries.forEach(function (name) {
    const color = tokenToolColor(name);
    ctx.fillStyle = color;
    ctx.beginPath();
    ctx.arc(lx + 5, y, 5, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = C.muted;
    ctx.font = "600 13px 'Segoe UI', sans-serif";
    ctx.fillText(name, lx + 14, y);
    const tw = ctx.measureText(name).width;
    lx += 14 + tw + 18;
  });
  ctx.textBaseline = "alphabetic";
}

// Distribution panel: a title + up to 5 rows (name + bar + amount/percent).
function drawOverviewDistPanel(ctx, C, x, y, w, panelH, title, values, useToolColor) {
  const padding = 16;
  drawShareCardSurface(ctx, x, y, w, panelH, padding, 22);
  const innerX = x + padding;
  const innerW = w - padding * 2;
  ctx.fillStyle = C.ink;
  ctx.font = "800 18px 'Segoe UI', sans-serif";
  ctx.textAlign = "left";
  ctx.fillText(title, innerX, y + padding + 18);
  const entries = Object.keys(values)
    .filter(function (k) { return values[k] > 0; })
    .sort(function (a, b) { return values[b] - values[a]; })
    .slice(0, 5);
  const total = entries.reduce(function (a, k) { return a + values[k]; }, 0) || 1;
  const rowStart = y + padding + 28 + 8;
  const rowH = 32;
  if (!entries.length) {
    ctx.fillStyle = C.muted;
    ctx.font = "600 14px 'Segoe UI', sans-serif";
    ctx.fillText(t("暂无数据"), innerX, rowStart + 16);
    return;
  }
  entries.forEach(function (name, i) {
    const tokens = values[name];
    const ry = rowStart + i * rowH;
    const fillW = Math.max(1, (tokens / total) * innerW);
    const color = useToolColor ? tokenToolColor(name) : C.green;
    // Name.
    ctx.fillStyle = C.ink;
    ctx.font = "700 14px 'Segoe UI', sans-serif";
    ctx.textAlign = "left";
    ctx.fillText(truncate(ctx, name, innerW * 0.55), innerX, ry + 14);
    // Track + fill.
    const trackY = ry + 18;
    ctx.fillStyle = C.track;
    ctx.beginPath();
    ctx.roundRect(innerX, trackY, innerW, 6, 3);
    ctx.fill();
    ctx.fillStyle = color;
    ctx.beginPath();
    ctx.roundRect(innerX, trackY, fillW, 6, 3);
    ctx.fill();
    // Amount + percent (right).
    ctx.fillStyle = C.muted;
    ctx.font = "600 13px 'Segoe UI', sans-serif";
    ctx.textAlign = "right";
    ctx.fillText(formatTokens(tokens, true) + " · " + (100 * tokens / total).toFixed(1) + "%", innerX + innerW, ry + 14);
  });
}

// Pill capsule (port of front-end .pill): label (muted) + value (ink).
// Draws a full-width rounded pill occupying (x,y,w,h).
function drawPill(ctx, x, y, w, h, label, value) {
  ctx.save();
  ctx.fillStyle = themeColors.surface;
  ctx.strokeStyle = "rgba(0,0,0,0.055)";
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.roundRect(x, y, w, h, h / 2);
  ctx.fill();
  ctx.stroke();
  ctx.textAlign = "left";
  ctx.textBaseline = "middle";
  const cy = y + h / 2;
  ctx.fillStyle = themeColors.muted;
  ctx.font = "600 13px 'Segoe UI', sans-serif";
  ctx.fillText(label, x + 16, cy);
  ctx.fillStyle = themeColors.ink;
  ctx.font = "700 16px 'Segoe UI', sans-serif";
  ctx.textAlign = "right";
  ctx.fillText(value, x + w - 16, cy);
  ctx.textBaseline = "alphabetic";
  ctx.restore();
}

// Lap-chip row (port of front-end .lap-chips). Draws chips left-aligned,
// wrapping not needed (≤2 chips). active chip text uses lapColor.
function drawLapChips(ctx, x, y, maxW, h, chips, lapColor) {
  ctx.save();
  ctx.textBaseline = "middle";
  const gap = 10;
  let cx = x;
  chips.forEach(function (chip) {
    // Measure title+detail to size the chip width.
    ctx.font = "800 13px 'Segoe UI', sans-serif";
    const tw = ctx.measureText(chip.title).width;
    ctx.font = "700 13px 'Segoe UI', sans-serif";
    const dw = ctx.measureText(chip.detail).width;
    const innerW = tw + 10 + dw;
    const cw = innerW + 24; // padding 12 each side
    // Background.
    if (chip.active) {
      ctx.fillStyle = hexA(themeColors.green, 0.12);
      ctx.strokeStyle = hexA(themeColors.green, 0.36);
    } else {
      ctx.fillStyle = hexA(themeColors.track, 0.46);
      ctx.strokeStyle = "rgba(0,0,0,0.055)";
    }
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.roundRect(cx, y, cw, h, 12);
    ctx.fill();
    ctx.stroke();
    // Text: title + detail inline, vertically centered.
    ctx.textAlign = "left";
    ctx.fillStyle = chip.active ? lapColor : themeColors.ink;
    ctx.font = "800 13px 'Segoe UI', sans-serif";
    ctx.fillText(chip.title, cx + 12, y + h / 2);
    ctx.fillStyle = chip.active ? lapColor : themeColors.muted;
    ctx.font = "700 13px 'Segoe UI', sans-serif";
    ctx.fillText(chip.detail, cx + 12 + tw + 10, y + h / 2);
    cx += cw + gap;
  });
  ctx.textBaseline = "alphabetic";
  ctx.restore();
}

// Metric strip: 3 equal-width mini cards (port of .metric-strip / .metric-mini).
function drawMetricStrip(ctx, x, y, w, h, items) {
  const gap = 18;
  const cardW = (w - gap * (items.length - 1)) / items.length;
  ctx.save();
  items.forEach(function (it, i) {
    const cx = x + i * (cardW + gap);
    // Card surface.
    ctx.fillStyle = themeColors.surface;
    ctx.strokeStyle = "rgba(0,0,0,0.055)";
    ctx.lineWidth = 1;
    ctx.save();
    ctx.shadowColor = "rgba(0,0,0,0.06)";
    ctx.shadowBlur = 14;
    ctx.shadowOffsetY = 6;
    ctx.beginPath();
    ctx.roundRect(cx, y, cardW, h, 24);
    ctx.fill();
    ctx.restore();
    ctx.beginPath();
    ctx.roundRect(cx, y, cardW, h, 24);
    ctx.stroke();
    // Label / value / detail, top-aligned with padding 22.
    const tx = cx + 22;
    ctx.textAlign = "left";
    ctx.textBaseline = "alphabetic";
    ctx.fillStyle = themeColors.muted;
    ctx.font = "700 15px 'Segoe UI', sans-serif";
    ctx.fillText(it.label, tx, y + 30);
    ctx.fillStyle = themeColors.ink;
    ctx.font = "800 27px 'Segoe UI', sans-serif";
    ctx.fillText(it.value, tx, y + 64);
    ctx.fillStyle = themeColors.muted;
    ctx.font = "600 13px 'Segoe UI', sans-serif";
    ctx.fillText(it.detail, tx, y + 84);
  });
  ctx.restore();
}

// Small shield-ish glyph for the footer (stand-in for SF "shield.checkered").
function drawShield(ctx, x, y, color) {
  ctx.save();
  ctx.strokeStyle = color;
  ctx.fillStyle = color;
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  ctx.moveTo(x + 7, y);
  ctx.lineTo(x + 14, y + 2);
  ctx.lineTo(x + 14, y + 8);
  ctx.quadraticCurveTo(x + 14, y + 14, x + 7, y + 16);
  ctx.quadraticCurveTo(x, y + 14, x, y + 8);
  ctx.lineTo(x, y + 2);
  ctx.closePath();
  ctx.globalAlpha = 0.18;
  ctx.fill();
  ctx.globalAlpha = 1;
  ctx.stroke();
  ctx.restore();
}

// Hex color (#rrggbb or rgb(r,g,b)) + alpha → rgba() string.
function hexA(hex, a) {
  const rgb = parseColor(hex);
  return "rgba(" + rgb[0] + "," + rgb[1] + "," + rgb[2] + "," + a + ")";
}

// Parse "#rrggbb" or "rgb(r,g,b)" → [r,g,b].
function parseColor(c) {
  if (c.charAt(0) === "#") {
    const h = c.slice(1);
    return [parseInt(h.substring(0, 2), 16), parseInt(h.substring(2, 4), 16), parseInt(h.substring(4, 6), 16)];
  }
  const m = c.match(/\d+/g);
  return [parseInt(m[0], 10), parseInt(m[1], 10), parseInt(m[2], 10)];
}

// Lighten a color toward white by `amt` (0..1) → "#rrggbb".
function lighten(c, amt) {
  const rgb = parseColor(c);
  const r = Math.round(rgb[0] + (255 - rgb[0]) * amt);
  const g = Math.round(rgb[1] + (255 - rgb[1]) * amt);
  const b = Math.round(rgb[2] + (255 - rgb[2]) * amt);
  return "#" + [r, g, b].map(function (v) { return ("0" + v.toString(16)).slice(-2); }).join("");
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

// Breakdown panel (port of ShareBreakdownPanel, compact:true).
// Title + subtitle header, then up to 4 rows: color dot + name (fixed width) +
// inline progress bar + value (fixed width), row height 25. Returns panel height.
function drawShareBreakdownPanel(ctx, C, x, y, w, title, subtitle, values, useToolColor) {
  const total = Math.max(dayValuesSum(values), 1);
  const entries = Object.keys(values)
    .filter(function (k) { return values[k] > 0; })
    .sort(function (a, b) { return values[b] - values[a]; })
    .slice(0, 4);
  const padding = 16;
  const radius = 22;
  const headerH = 34; // title + subtitle
  const rowH = 25;
  const panelH = padding * 2 + headerH + Math.max(entries.length, 1) * rowH;
  // Surface (shadow + stroke).
  drawShareCardSurface(ctx, x, y, w, panelH, padding, radius);
  const innerX = x + padding;
  const innerW = w - padding * 2;
  // Title.
  ctx.fillStyle = C.ink;
  ctx.font = "800 16px 'Segoe UI', sans-serif";
  ctx.textAlign = "left";
  ctx.fillText(title, innerX, y + padding + 14);
  // Subtitle.
  ctx.fillStyle = C.muted;
  ctx.font = "700 12px 'Segoe UI', sans-serif";
  ctx.fillText(subtitle, innerX, y + padding + 30);
  // Rows: dot + name(104) + bar(flex) + value(72).
  const nameW = 104;
  const valueW = 72;
  const gap = 10;
  const dotR = 3.5;
  const rowStart = y + padding + headerH + 12;
  entries.forEach(function (name, i) {
    const tokens = values[name];
    const pct = (tokens / total) * 100;
    const ry = rowStart + i * rowH;
    const rowCenter = ry + rowH / 2;
    const color = useToolColor ? tokenToolColor(name) : C.green;
    // Color dot.
    ctx.fillStyle = color;
    ctx.beginPath();
    ctx.arc(innerX + dotR, rowCenter, dotR, 0, Math.PI * 2);
    ctx.fill();
    // Name (fixed width, left-aligned).
    ctx.fillStyle = hexA(C.ink, 0.76);
    ctx.font = "700 13px 'Segoe UI', sans-serif";
    ctx.textAlign = "left";
    ctx.fillText(truncate(ctx, name, nameW - gap), innerX + dotR * 2 + gap, rowCenter + 4);
    // Inline progress bar (flex between name and value).
    const barX = innerX + dotR * 2 + gap + nameW + gap;
    const barW = innerW - (dotR * 2 + gap + nameW + gap) - valueW - gap;
    ctx.fillStyle = C.track;
    ctx.beginPath();
    ctx.roundRect(barX, rowCenter - 3.5, barW, 7, 3.5);
    ctx.fill();
    ctx.fillStyle = color;
    ctx.beginPath();
    ctx.roundRect(barX, rowCenter - 3.5, Math.max(5, (barW * pct) / 100), 7, 3.5);
    ctx.fill();
    // Value (fixed width, right-aligned, monospace-ish digits).
    ctx.fillStyle = C.muted;
    ctx.font = "800 13px 'Segoe UI', sans-serif";
    ctx.textAlign = "right";
    ctx.fillText(formatTokens(tokens, true), innerX + innerW, rowCenter + 4);
  });
  if (!entries.length) {
    ctx.fillStyle = C.mutedFaint;
    ctx.font = "700 13px 'Segoe UI', sans-serif";
    ctx.textAlign = "center";
    ctx.fillText(t("暂无数据"), x + w / 2, rowStart + 12);
  }
  return panelH;
}

// Truncate a string with ellipsis if wider than maxW using current font.
function truncate(ctx, str, maxW) {
  if (ctx.measureText(str).width <= maxW) return str;
  let s = str;
  while (s.length > 1 && ctx.measureText(s + "…").width > maxW) {
    s = s.slice(0, -1);
  }
  return s + "…";
}

function dayValuesSum(values) {
  var s = 0;
  for (var k in values) s += Number(values[k] || 0);
  return s;
}

// 30-day trend (port of ShareTrendPanel): title + subtitle + summary capsule +
// stacked mini bars. Returns panel height. Bars use macOS metrics:
// StackedActivityBarsView gap=5, cornerRadius=min(4,w/2), height 84.
function drawShareTrendPanel(ctx, C, x, y, w, daily, goal, day) {
  const padding = 16;
  const radius = 22;
  const panelH = 162; // padding*2 + header(34) + gap(12) + bars(84)
  drawShareCardSurface(ctx, x, y, w, panelH, padding, radius);
  const innerX = x + padding;
  const innerW = w - padding * 2;
  // Title + subtitle (left).
  ctx.fillStyle = C.ink;
  ctx.font = "800 16px 'Segoe UI', sans-serif";
  ctx.textAlign = "left";
  ctx.fillText(t("最近 30 天"), innerX, y + padding + 14);
  ctx.fillStyle = C.muted;
  ctx.font = "700 12px 'Segoe UI', sans-serif";
  ctx.fillText(t("柱越高，用量越多"), innerX, y + padding + 30);
  // Summary capsule (right): today's tokens on mint background.
  const capText = formatTokens((day && day.total_tokens) || 0, true);
  ctx.font = "800 15px 'Segoe UI', sans-serif";
  const capW = ctx.measureText(capText).width + 20;
  const capH = 28;
  const capX = x + w - padding - capW;
  const capY = y + padding + 2;
  ctx.fillStyle = hexA(C.mint, 0.24);
  ctx.beginPath();
  ctx.roundRect(capX, capY, capW, capH, capH / 2);
  ctx.fill();
  ctx.fillStyle = C.greenDark;
  ctx.textAlign = "center";
  ctx.fillText(capText, capX + capW / 2, capY + capH - 8);

  const rows = daily.slice(-30);
  if (!rows.length) return panelH;
  const maxTokens = Math.max.apply(null, [goal].concat(rows.map(function (d) { return d.total_tokens; })).concat([1]));
  const barsY = y + padding + 46;
  const barsH = 84; // macOS StackedActivityBarsView .frame(height:84)
  const barGap = 5; // macOS gap
  const barW = Math.max(4, (innerW - barGap * (rows.length - 1)) / rows.length);
  const barR = Math.min(4, barW / 2);
  rows.forEach(function (d, i) {
    const bx = innerX + i * (barW + barGap);
    const totalH = Math.max(4, (d.total_tokens / maxTokens) * barsH);
    // Empty day → 4pt track-colored placeholder bar at the bottom (macOS behavior).
    if (d.total_tokens <= 0) {
      ctx.fillStyle = C.track;
      ctx.beginPath();
      ctx.roundRect(bx, barsY + barsH - 4, barW, 4, Math.min(2, barW / 2));
      ctx.fill();
      return;
    }
    const segs = orderedToolEntries(d.tools || {});
    if (!segs.length) {
      // No tool breakdown → single contribution-color bar.
      ctx.fillStyle = contributionColor(d.total_tokens, goal);
      ctx.beginPath();
      ctx.roundRect(bx, barsY + barsH - totalH, barW, totalH, barR);
      ctx.fill();
      return;
    }
    // Stacked by tool, segments.reversed() (largest at bottom). The whole bar
    // is clipped to the rounded shape so top/bottom corners are round.
    ctx.save();
    ctx.beginPath();
    ctx.roundRect(bx, barsY + barsH - totalH, barW, totalH, barR);
    ctx.clip();
    let drawn = 0;
    segs.slice().reverse().forEach(function (s) {
      const sh = Math.max(1, (totalH * s.tokens) / Math.max(d.total_tokens, 1));
      ctx.fillStyle = tokenToolColor(s.name);
      ctx.fillRect(bx, barsY + barsH - drawn - sh, barW, sh);
      drawn += sh;
    });
    ctx.restore();
  });
  // Goal reference line at goal/maxTokens (port of macOS 1pt .quaternary line).
  const goalLineY = barsY + barsH - (goal / maxTokens) * barsH;
  ctx.strokeStyle = hexA(C.muted, 0.45);
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(innerX, goalLineY);
  ctx.lineTo(innerX + innerW, goalLineY);
  ctx.stroke();
  return panelH;
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

  // 1) Neon backdrop: linear 3-stop + accent radial (bottomLeading) +
  //    secondary radial (topTrailing) + decorative slanted shape + grid.
  ctx.save();
  // Base linear gradient (topLeading → bottomTrailing).
  const bg = ctx.createLinearGradient(0, 0, W, H);
  bg.addColorStop(0, palette.bg0);
  bg.addColorStop(0.5, palette.bg1);
  bg.addColorStop(1, palette.bg2);
  ctx.fillStyle = bg;
  ctx.fillRect(0, 0, W, H);
  // Accent radial glow from bottom-left.
  const rad1 = ctx.createRadialGradient(20, H, 20, 20, H, 440);
  rad1.addColorStop(0, hexA(palette.accent, 0.26));
  rad1.addColorStop(1, "rgba(0,0,0,0)");
  ctx.fillStyle = rad1;
  ctx.fillRect(0, 0, W, H);
  // Secondary radial glow from top-right.
  const rad2 = ctx.createRadialGradient(W, 0, 40, W, 0, 380);
  rad2.addColorStop(0, hexA(palette.secondary, 0.16));
  rad2.addColorStop(1, "rgba(0,0,0,0)");
  ctx.fillStyle = rad2;
  ctx.fillRect(0, 0, W, H);
  ctx.restore();
  // Decorative slanted translucent slab (rotated rect near top-right).
  ctx.save();
  ctx.translate(W - 80, 60);
  ctx.rotate((-23 * Math.PI) / 180);
  ctx.fillStyle = hexA(palette.accent, 0.10);
  ctx.fillRect(-180, -190, 360, 380);
  ctx.restore();
  // Grid: 11 columns × 14 rows.
  ctx.strokeStyle = "rgba(255,255,255,0.026)";
  ctx.lineWidth = 1;
  for (var i = 1; i < 11; i++) {
    const gx = (W * i) / 11;
    ctx.beginPath();
    ctx.moveTo(gx, 0);
    ctx.lineTo(gx, H);
    ctx.stroke();
  }
  for (var j = 1; j < 14; j++) {
    const gy = (H * j) / 14;
    ctx.beginPath();
    ctx.moveTo(0, gy);
    ctx.lineTo(W, gy);
    ctx.stroke();
  }

  const padX = 30;
  const padTop = 26;
  let y = padTop;

  // 2) Header: brand mark(50) + glow + title/subtitle (left) | date+weekday (right).
  ctx.save();
  ctx.shadowColor = hexA(palette.accent, 0.22);
  ctx.shadowBlur = 12;
  drawTokenStepMark(ctx, padX, y, 50, { surface: "#ffffff" });
  ctx.restore();
  ctx.textAlign = "left";
  ctx.fillStyle = "#ffffff";
  ctx.font = "800 23px 'Segoe UI', sans-serif";
  ctx.fillText("TokenStep", padX + 50 + 13, y + 24);
  ctx.fillStyle = "rgba(255,255,255,0.55)";
  ctx.font = "600 14px 'Segoe UI', sans-serif";
  ctx.fillText(t("AI Token 使用追踪"), padX + 50 + 13, y + 42);
  // Right: date (accent) + weekday.
  ctx.textAlign = "right";
  ctx.fillStyle = palette.accent;
  ctx.font = "700 15px 'Segoe UI', sans-serif";
  const dateStr = formatRhythmDate(opts.date);
  ctx.fillText(dateStr, W - padX, y + 24);
  ctx.fillStyle = "rgba(255,255,255,0.56)";
  ctx.font = "700 15px 'Segoe UI', sans-serif";
  ctx.fillText(formatRhythmWeekday(opts.date), W - padX, y + 42);

  y += 50 + 14; // header height + spacing

  // 3) Hero: "昨日 AI 节奏" title + laurel-flanked gradient tag + shareLine sub.
  ctx.textAlign = "center";
  ctx.fillStyle = "#ffffff";
  ctx.font = "800 31px 'Segoe UI', sans-serif";
  ctx.fillText(t("昨日 AI 节奏"), W / 2, y + 24);
  const tagY = y + 24 + 56;
  // Laurel branches flanking the tag.
  drawLaurel(ctx, W / 2 - 150, tagY - 8, -1, palette.accent, 0.78);
  drawLaurel(ctx, W / 2 + 150, tagY - 8, 1, palette.accent, 0.78);
  // Tag text with accent→secondary gradient + accent glow shadow.
  ctx.save();
  ctx.shadowColor = hexA(palette.accent, 0.34);
  ctx.shadowBlur = 16;
  ctx.shadowOffsetY = 4;
  const tagGrad = ctx.createLinearGradient(W / 2 - 130, tagY, W / 2 + 130, tagY);
  tagGrad.addColorStop(0, palette.accent);
  tagGrad.addColorStop(1, palette.secondary);
  ctx.fillStyle = tagGrad;
  ctx.font = "800 43px 'Segoe UI', sans-serif";
  ctx.textAlign = "center";
  ctx.fillText(rhythmTagTitle(tag), W / 2, tagY);
  ctx.restore();
  // shareLine under the tag.
  ctx.fillStyle = "rgba(255,255,255,0.66)";
  ctx.font = "700 15px 'Segoe UI', sans-serif";
  ctx.fillText(rhythmShareLine(tag), W / 2, tagY + 28);

  y = tagY + 28 + 16;

  // 4) Neon waveform panel (inline; the wave draws directly on the backdrop).
  const waveH = 188;
  const waveX = padX;
  const waveW = W - padX * 2;
  drawRhythmWave(ctx, rhythm, waveX, y, waveW, waveH, palette);

  // 5) Hour axis with moon/sun icons at the ends.
  const axisY = y + waveH + 6;
  drawRhythmAxis(ctx, waveX, axisY, waveW, palette);

  y = axisY + 30;

  // 6) Token console: dark capsule with chevron clusters + big gradient number.
  const consoleH = 80;
  drawChevronCluster(ctx, padX + 8, y + consoleH / 2, -1, palette);
  drawChevronCluster(ctx, W - padX - 8, y + consoleH / 2, 1, palette);
  // Dark capsule background.
  ctx.fillStyle = "rgba(0,0,0,0.24)";
  ctx.beginPath();
  ctx.roundRect(padX, y, W - padX * 2, consoleH, 18);
  ctx.fill();
  ctx.strokeStyle = hexA(palette.secondary, 0.24);
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.roundRect(padX, y, W - padX * 2, consoleH, 18);
  ctx.stroke();
  // Label + big number (accent→white gradient).
  ctx.fillStyle = "rgba(255,255,255,0.70)";
  ctx.font = "800 17px 'Segoe UI', sans-serif";
  ctx.textAlign = "center";
  ctx.fillText(t("昨日 Token"), W / 2, y + 26);
  ctx.save();
  ctx.shadowColor = hexA(palette.accent, 0.30);
  ctx.shadowBlur = 16;
  ctx.shadowOffsetY = 4;
  const numGrad = ctx.createLinearGradient(0, y + 30, 0, y + consoleH - 6);
  numGrad.addColorStop(0, palette.accent);
  numGrad.addColorStop(1, "#ffffff");
  ctx.fillStyle = numGrad;
  ctx.font = "800 42px 'Segoe UI', sans-serif";
  ctx.fillText(formatTokens(rhythm.total_tokens || 0), W / 2, y + consoleH - 12);
  ctx.restore();

  y += consoleH + 14;

  // 7) Three metrics row with vertical dividers.
  const metrics = rhythmMetrics(rhythm);
  const mRowH = 64;
  const mW = (W - padX * 2) / metrics.length;
  metrics.forEach(function (m, i) {
    const mx = padX + i * mW;
    const center = mx + mW / 2;
    const mColor = m.color || palette.accent;
    // Divider between metrics.
    if (i > 0) {
      ctx.strokeStyle = "rgba(255,255,255,0.20)";
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(mx, y + 4);
      ctx.lineTo(mx, y + mRowH - 12);
      ctx.stroke();
    }
    // Glyph dot + label.
    ctx.fillStyle = mColor;
    ctx.beginPath();
    ctx.arc(center - ctx.measureText(m.label).width / 2 - 8, y + 14, 3.5, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = "rgba(255,255,255,0.78)";
    ctx.font = "700 12px 'Segoe UI', sans-serif";
    ctx.textAlign = "center";
    ctx.fillText(m.label, center + 6, y + 18);
    // Value.
    ctx.fillStyle = "#ffffff";
    ctx.font = "800 22px 'Segoe UI', sans-serif";
    ctx.fillText(m.value, center, y + 48);
  });

  y += mRowH;

  // 8) Footer: lock + 本地统计·不上传对话 in a dark capsule.
  const footerY = H - padTop - 10;
  const footerText = t("本地统计") + " · " + t("不上传对话");
  ctx.font = "700 12px 'Segoe UI', sans-serif";
  const footW = ctx.measureText(footerText).width + 44;
  ctx.fillStyle = "rgba(0,0,0,0.20)";
  ctx.beginPath();
  ctx.roundRect(W / 2 - footW / 2, footerY - 14, footW, 28, 14);
  ctx.fill();
  // Lock glyph (simplified).
  ctx.strokeStyle = "rgba(255,255,255,0.54)";
  ctx.fillStyle = "rgba(255,255,255,0.54)";
  ctx.lineWidth = 1.3;
  const lockX = W / 2 - footW / 2 + 14;
  ctx.beginPath();
  ctx.arc(lockX, footerY - 4, 3, Math.PI, 0);
  ctx.stroke();
  ctx.beginPath();
  ctx.roundRect(lockX - 4, footerY - 4, 8, 7, 1.5);
  ctx.fill();
  ctx.fillStyle = "rgba(255,255,255,0.54)";
  ctx.textAlign = "left";
  ctx.fillText(footerText, lockX + 10, footerY + 1);

  return canvas;
}

// Format date as "2026.06.23" for the rhythm header (matches Mac yyyy.MM.dd).
function formatRhythmDate(dateStr) {
  if (!dateStr) return "";
  return dateStr.replace(/-/g, ".");
}

// Format weekday as "周一" (zh) / "Mon" (en) for the rhythm header.
function formatRhythmWeekday(dateStr) {
  if (!dateStr) return "";
  const d = new Date(dateStr + "T00:00:00");
  if (isNaN(d.getTime())) return "";
  const idx = d.getDay();
  if (t("周日") === "Sunday") {
    const en = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    return en[idx];
  }
  const zh = [t("周日"), t("周一"), t("周二"), t("周三"), t("周四"), t("周五"), t("周六")];
  return zh[idx];
}

// Laurel branch (port of LaurelBranch): quad-curve stem + 5 leaf lobes.
// `dir` -1 left / 1 right. Drawn at the BASE of the stem (x,y).
function drawLaurel(ctx, x, y, dir, color, alpha) {
  ctx.save();
  ctx.translate(x, y);
  ctx.scale(dir, 1);
  ctx.strokeStyle = hexA(color, alpha != null ? alpha : 0.78);
  ctx.fillStyle = hexA(color, alpha != null ? alpha : 0.78);
  ctx.lineWidth = 2;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  // Stem: quad curve from base (0.64W, H-4) to tip (W-3, 8).
  ctx.beginPath();
  ctx.moveTo(0.64 * 38, 56 - 4);
  ctx.quadraticCurveTo(38 - 7, 28, 38 - 3, 8);
  ctx.stroke();
  // 5 leaves along the stem, each drawn as two quad curves (a lobe).
  for (var i = 0; i < 5; i++) {
    const t1 = (i + 1) / 6;
    // Approximate stem point at parameter t1 by sampling the quad curve.
    const sx = quadX(0.64 * 38, 38 - 7, 38 - 3, t1);
    const sy = quadY(56 - 4, 28, 8, t1);
    const leafLen = 11;
    const leafOff = 4 + i * 2.2; // outward horizontal offset
    ctx.beginPath();
    ctx.moveTo(sx, sy);
    ctx.quadraticCurveTo(sx + leafOff, sy - leafLen * 0.6, sx + leafOff, sy - leafLen);
    ctx.quadraticCurveTo(sx + leafOff - 2, sy - leafLen * 0.6, sx, sy + 1);
    ctx.fill();
  }
  ctx.restore();
}

// Quadratic Bézier point at parameter t.
function quadX(p0, p1, p2, t) {
  const u = 1 - t;
  return u * u * p0 + 2 * u * t * p1 + t * t * p2;
}
function quadY(p0, p1, p2, t) { return quadX(p0, p1, p2, t); }

// 4 palettes keyed by rhythm tag — Mac-native RhythmCardPalette color values.
// bg = 3-stop background gradient; accent/secondary/night = neon; panel = dark surface.
// `glow` is derived as accent@full for the neon glow (Mac uses accent for blur).
function rhythmPalette(tag) {
  switch (tag) {
    case "night_agent":
      return {
        bg0: "#020611", bg1: "#030E19", bg2: "#080C23",
        accent: "#4AF78B", secondary: "#3EC0FF", night: "#2D6CFF", panel: "#04262B",
        glow: "#4AF78B", accent2: "#3EC0FF",
      };
    case "morning_planner":
    case "early_starter":
      return {
        bg0: "#030F0C", bg1: "#04231B", bg2: "#1F1E0D",
        accent: "#55F697", secondary: "#FFBE2C", night: "#2F98FF", panel: "#042922",
        glow: "#55F697", accent2: "#FFBE2C",
      };
    case "fragmented":
    case "double_peak":
      return {
        bg0: "#050812", bg1: "#041C1B", bg2: "#140C2A",
        accent: "#50F690", secondary: "#2ED6FF", night: "#695CFF", panel: "#05252A",
        glow: "#50F690", accent2: "#2ED6FF",
      };
    default:
      return {
        bg0: "#010A07", bg1: "#031611", bg2: "#02131F",
        accent: "#4FF48A", secondary: "#31CDFF", night: "#2968FF", panel: "#032629",
        glow: "#4FF48A", accent2: "#31CDFF",
      };
  }
}

// Neon waveform (port of RhythmNeonWavePanel). x,y = top-left of the chart box,
// w×h = chart area. Draws: grid, area fill, blurred glow line, crisp gradient
// line, and a dashed peak marker with a white dot + colored ring.
function drawRhythmWave(ctx, rhythm, x, y, w, h, palette) {
  const buckets = rhythm.buckets || [];
  if (!buckets.length) return;
  // Smoothed + gamma-normalized values (port of the `values` computed property).
  const values = smoothRhythmValues(buckets);
  const max = Math.max.apply(null, values.concat([0.001]));
  const stepX = w / (values.length - 1);
  // Chart y-mapping (Mac: maxY - 20 - v*(height-42), here scaled to h).
  const topPad = h * 0.10;
  const botPad = 16;
  const points = values.map(function (v, i) {
    const norm = Math.max(v / max, 0.04);
    return {
      x: x + i * stepX,
      y: y + h - botPad - norm * (h - topPad - botPad),
      v: v,
    };
  });

  // Area fill under the curve.
  ctx.save();
  ctx.beginPath();
  ctx.moveTo(points[0].x, y + h - 8);
  catmullRomPath(ctx, points, 1 / 6);
  ctx.lineTo(points[points.length - 1].x, y + h - 8);
  ctx.closePath();
  const areaGrad = ctx.createLinearGradient(0, y, 0, y + h);
  areaGrad.addColorStop(0, hexA(palette.accent, 0.48));
  areaGrad.addColorStop(0.5, hexA(palette.secondary, 0.22));
  areaGrad.addColorStop(1, "rgba(0,0,0,0)");
  ctx.fillStyle = areaGrad;
  ctx.fill();
  ctx.restore();

  // Blurred glow line (secondary @ 0.38, wide, blurred).
  ctx.save();
  ctx.shadowColor = hexA(palette.secondary, 0.5);
  ctx.shadowBlur = 10;
  ctx.strokeStyle = hexA(palette.secondary, 0.38);
  ctx.lineWidth = 14;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  ctx.beginPath();
  ctx.moveTo(points[0].x, points[0].y);
  catmullRomPath(ctx, points, 1 / 6);
  ctx.stroke();
  ctx.restore();

  // Crisp line (accent → secondary → night gradient).
  ctx.save();
  const lineGrad = ctx.createLinearGradient(x, y, x + w, y);
  lineGrad.addColorStop(0, palette.accent);
  lineGrad.addColorStop(0.5, palette.secondary);
  lineGrad.addColorStop(1, palette.night);
  ctx.strokeStyle = lineGrad;
  ctx.lineWidth = 5;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  ctx.beginPath();
  ctx.moveTo(points[0].x, points[0].y);
  catmullRomPath(ctx, points, 1 / 6);
  ctx.stroke();
  ctx.restore();

  // Peak marker: dashed vertical line + white dot + colored ring.
  if (rhythm.peak_hour != null) {
    const peak = points[rhythm.peak_hour];
    if (peak) {
      ctx.save();
      // Dashed line from peak down to baseline.
      ctx.strokeStyle = hexA(palette.secondary, 0.36);
      ctx.lineWidth = 1.5;
      ctx.setLineDash([5, 7]);
      ctx.beginPath();
      ctx.moveTo(peak.x, peak.y + 4);
      ctx.lineTo(peak.x, y + h - 8);
      ctx.stroke();
      ctx.setLineDash([]);
      // Glow ring.
      ctx.shadowColor = hexA(palette.secondary, 0.8);
      ctx.shadowBlur = 14;
      ctx.fillStyle = "#ffffff";
      ctx.beginPath();
      ctx.arc(peak.x, peak.y, 5.5, 0, Math.PI * 2);
      ctx.fill();
      ctx.shadowBlur = 0;
      ctx.strokeStyle = palette.secondary;
      ctx.lineWidth = 3;
      ctx.beginPath();
      ctx.arc(peak.x, peak.y, 5.5, 0, Math.PI * 2);
      ctx.stroke();
      ctx.restore();
    }
  }
}

// Smoothed + gamma-normalized hourly values (port of RhythmNeonWavePanel `values`).
// Weighted kernel 0.78 center + 0.09 each neighbor + 0.02 each 2nd neighbor,
// then pow(norm, 0.68) gamma, clamp [0.08, 1]; zero buckets → 0.04.
function smoothRhythmValues(buckets) {
  const raw = buckets.map(function (b) { return Number(b && b.tokens) || 0; });
  const smoothed = raw.map(function (_, i) {
    const c = raw[i] || 0;
    const n1 = (raw[i - 1] || 0) + (raw[i + 1] || 0);
    const n2 = (raw[i - 2] || 0) + (raw[i + 2] || 0);
    return 0.78 * c + 0.09 * n1 + 0.02 * n2;
  });
  const max = Math.max.apply(null, smoothed.concat([0.0001]));
  return smoothed.map(function (v) {
    if (v <= 0) return 0.04;
    const norm = v / max;
    const gamma = Math.pow(norm, 0.68);
    return Math.max(0.08, Math.min(1, gamma));
  });
}

// Chevron cluster: 4 chevrons pointing toward dir (-1 left / 1 right).
// Color sequence mirrors Mac RhythmChevronCluster.
function drawChevronCluster(ctx, x, y, dir, palette) {
  ctx.save();
  ctx.translate(x, y);
  ctx.scale(dir, 1);
  const colors = [
    hexA(palette.accent, 0.25),
    palette.secondary,
    hexA(palette.accent, 0.54),
    hexA(palette.accent, 0.25),
  ];
  ctx.shadowColor = hexA(palette.accent, 0.34);
  ctx.shadowBlur = 9;
  for (var i = 0; i < 4; i++) {
    ctx.strokeStyle = colors[i];
    ctx.lineWidth = 3.5;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    const cx = -i * 9; // stack toward the center
    ctx.beginPath();
    ctx.moveTo(cx + 4, -8);
    ctx.lineTo(cx, 0);
    ctx.lineTo(cx + 4, 8);
    ctx.stroke();
  }
  ctx.restore();
}

// shareLine for a rhythm tag (subtitle under the hero tag). Falls back to a
// generic line if the tag isn't in the table.
function rhythmShareLine(tag) {
  const map = {
    night_agent: t("夜深了还在和 AI 并肩作战"),
    morning_planner: t("清晨就开始规划今日工作"),
    early_starter: t("清晨就开始规划今日工作"),
    afternoon_burst: t("下午高效推进任务"),
    evening_sprint: t("晚间冲刺完成收尾"),
    double_peak: t("上下午两个高效时段"),
    fragmented: t("碎片化时间持续输出"),
    one_shot: t("一次集中爆发完成"),
    steady_cruise: t("全天平稳持续输出"),
    quiet_day: t("今天节奏比较舒缓"),
  };
  return map[tag] || map.quiet_day;
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

// Hour axis: 0/6/12/18/24 with moon/sun icons at 0 and 24 (port of RhythmAxisLabel).
function drawRhythmAxis(ctx, x, y, w, palette) {
  const marks = [0, 6, 12, 18, 24];
  ctx.font = "700 11px 'Segoe UI', sans-serif";
  ctx.textAlign = "center";
  marks.forEach(function (h) {
    const px = x + (h / 24) * w;
    if (h === 0) {
      // Moon (night color) + "0时".
      ctx.fillStyle = palette.night;
      drawAxisMoon(ctx, px - 18, y - 2, palette.night);
      ctx.fillStyle = "rgba(255,255,255,0.48)";
      ctx.fillText(h + t("时"), px + 2, y + 12);
    } else if (h === 12) {
      ctx.fillStyle = "#FFB820";
      drawAxisSun(ctx, px - 7, y - 5, "#FFB820");
      ctx.fillStyle = "rgba(255,255,255,0.48)";
      ctx.fillText(h + t("时"), px + 8, y + 12);
    } else if (h === 24) {
      ctx.fillStyle = palette.night;
      drawAxisMoon(ctx, px + 6, y - 2, palette.night);
      ctx.fillStyle = "rgba(255,255,255,0.48)";
      ctx.fillText(t("24时"), px - 6, y + 12);
    } else {
      ctx.fillStyle = "rgba(255,255,255,0.48)";
      ctx.fillText(h + t("时"), px, y + 12);
    }
  });
}

// Small moon glyph for the axis (stand-in for SF "moon.stars.fill").
function drawAxisMoon(ctx, x, y, color) {
  ctx.save();
  ctx.fillStyle = color;
  ctx.beginPath();
  ctx.arc(x + 6, y + 6, 5, 0, Math.PI * 2);
  ctx.fill();
  ctx.globalCompositeOperation = "destination-out";
  ctx.beginPath();
  ctx.arc(x + 9, y + 4, 4.5, 0, Math.PI * 2);
  ctx.fill();
  ctx.restore();
}
// Small sun glyph for the axis (stand-in for SF "sun.max.fill").
function drawAxisSun(ctx, x, y, color) {
  ctx.save();
  ctx.fillStyle = color;
  ctx.beginPath();
  ctx.arc(x + 6, y + 6, 4, 0, Math.PI * 2);
  ctx.fill();
  for (var a = 0; a < 8; a++) {
    const ang = (a * Math.PI) / 4;
    ctx.beginPath();
    ctx.moveTo(x + 6 + Math.cos(ang) * 5.5, y + 6 + Math.sin(ang) * 5.5);
    ctx.lineTo(x + 6 + Math.cos(ang) * 8, y + 6 + Math.sin(ang) * 8);
    ctx.strokeStyle = color;
    ctx.lineWidth = 1.2;
    ctx.stroke();
  }
  ctx.restore();
}

// Three rhythm metrics (port of bottomMetrics). Each carries a `color`.
function rhythmMetrics(rhythm) {
  var palette = rhythmPalette(rhythm.primary_tag || "quiet_day");
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
    { label: t("活跃时段"), value: span + ":00", color: palette.accent },
    { label: t("夜间占比"), value: nightPct + "%", color: palette.night },
    { label: t("最长连续"), value: maxStreak + t("h"), color: palette.accent },
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
  // Legend swatches mirror macOS ContributionWallView: goal-relative 6-step ramp.
  const legendVals = [0, Math.round(goal * 0.25), Math.round(goal * 0.7), goal, goal * 2, goal * 3];
  return (
    '<div class="wall" id="activityWall">' + cols + "</div>" +
    '<div style="display:flex;gap:12px;flex-wrap:wrap;margin-top:14px">' +
    '<span class="pill"><span class="pill-label">' + t("活跃") + '</span><span class="pill-value">' + activeCount + t(" 天") + "</span></span>" +
    '<span class="pill"><span class="pill-label">' + t("达标") + '</span><span class="pill-value">' + goalCount + t(" 天") + "</span></span>" +
    '<span class="pill"><span class="pill-label">' + t("最高") + '</span><span class="pill-value">' + formatTokens(maxTokens, true) + "</span></span>" +
    "</div>" +
    '<div style="display:flex;align-items:center;gap:6px;margin-top:10px;font-size:12px;color:var(--muted);font-weight:600"><span>' + t("少") + '</span>' + legendVals.map(function (v) { return '<span style="width:12px;height:12px;border-radius:3px;background:' + contributionColor(v, goal) + '"></span>'; }).join("") + '<span>' + t("多") + '</span></div>'
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
// Each day is one vertical bar, split into colored segments per tool. The
// .activity container is a fixed-height track (96px, like macOS frame:84+
// breathing room); every bar is absolutely pinned to the bottom (bottom:0) so
// all bars share one baseline regardless of height — this is what makes the
// bottoms line up (was broken when bars relied on flex percentages).
function stackedActivityBarsHTML(rows, goal) {
  if (!rows || !rows.length) return '<div class="empty">' + t("暂无活动数据") + "</div>";
  const maxTokens = Math.max.apply(
    null,
    [goal].concat(rows.map((d) => d.total_tokens)).concat([1])
  );
  // Goal reference line at goal/maxTokens (port of macOS 1pt .quaternary line).
  const goalPct = Math.min(100, (goal / maxTokens) * 100);
  return (
    '<div class="activity" style="margin-top:14px">' +
    '<div class="goal-line" style="bottom:' + goalPct + '%"></div>' +
    rows
      .map((d) => {
        // Bar height as a fraction of the 96px track.
        const totalPct = (d.total_tokens / maxTokens) * 100;
        if (d.total_tokens <= 0) {
          // Empty day → 4px track-colored placeholder pinned to the bottom.
          return '<div class="bar" style="height:4px"></div>';
        }
        const segments = orderedToolEntries(d.tools || {});
        if (!segments.length) {
          return (
            '<div class="bar" style="height:' +
            totalPct +
            "%;background:" +
            contributionColor(d.total_tokens, goal) +
            '"></div>'
          );
        }
        // Segments stacked bottom-up: each segment's height is its share of
        // total_tokens *relative to the bar* (so the segments sum to exactly
        // 100% of the bar height — no overflow/underflow, bottoms always flush).
        // column-reverse stacks the first segment at the bottom.
        const segHtml = segments
          .map((s) => {
            const sharePct = (s.tokens / Math.max(d.total_tokens, 1)) * 100;
            return (
              '<div style="width:100%;height:' +
              sharePct +
              "%;background:" +
              tokenToolColor(s.name) +
              '"></div>'
            );
          })
          .join("");
        return '<div class="bar" style="height:' + totalPct + "%;display:flex;flex-direction:column-reverse\">" + segHtml + "</div>";
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
    // Goal-line legend (port of macOS TokenToolLegend showsGoalLine).
    '<span style="display:inline-flex;align-items:center;gap:5px;font-size:13px;color:var(--muted);font-weight:600">' +
    '<span style="width:16px;height:1px;background:var(--mutedFaint);opacity:.55"></span>' +
    t("每日目标") +
    "</span>" +
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
  renderTodayOverview,
  renderRhythmCard,
  rhythmMetrics,
  comparisonText,
};
