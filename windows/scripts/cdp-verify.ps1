# TokenStep CDP end-to-end verification (reusable).
# Launches the real app with WebView2 remote debugging, then checks:
#   sync -> today hero/logo -> history tab -> privacy tab (+ path text)
#   -> settings opens -> real theme switch + restore -> cache size on disk.
# Rules: ASCII-only source (Chinese via \uXXXX), never cancel ReceiveAsync.
param(
  [int]$Port = 9223,
  [string]$Exe = 'E:\Program\TokenStep\tokenstep.exe',
  [int]$CollectTimeoutSec = 480
)

function Test-PortFree([int]$p) {
  $c = New-Object System.Net.Sockets.TcpClient
  try { $c.Connect('127.0.0.1', $p); $c.Close(); return $false } catch { return $true }
}
foreach ($p in 9223, 9224, 9225, 9226) { if (Test-PortFree $p) { $Port = $p; break } }
Write-Output "debug port: $Port"

Get-Process tokenstep -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1
Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe'" |
  Where-Object { $_.CommandLine -match 'TokenStep' } |
  ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
Start-Sleep -Seconds 1

$cachePath = Join-Path $env:APPDATA 'TokenStep\cache\collector-cache.json'
$cacheBefore = if (Test-Path $cachePath) { (Get-Item $cachePath).Length } else { 0 }
Write-Output ("cache size before launch: " + $cacheBefore + " bytes")

$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port"
Start-Process -FilePath $Exe
Start-Sleep -Seconds 3
$targets = $null
for ($i = 0; $i -lt 15; $i++) {
  try { $targets = Invoke-RestMethod "http://127.0.0.1:$Port/json" -TimeoutSec 2; break } catch { Start-Sleep -Seconds 2 }
}
if (-not $targets) { Write-Output 'DEBUG PORT NEVER OPENED'; Get-Process tokenstep -ErrorAction SilentlyContinue | Stop-Process -Force; exit 1 }

$ws = [System.Net.WebSockets.ClientWebSocket]::new()
$page = $targets | Where-Object { $_.type -eq 'page' } | Select-Object -First 1
$ws.ConnectAsync([Uri]$page.webSocketDebuggerUrl, [Threading.CancellationToken]::None).Wait(15000)

$script:id = 0
function Invoke-Eval([string]$js, [int]$timeoutMs = 20000) {
  $script:id++
  $myId = $script:id
  $payload = @{ id = $myId; method = 'Runtime.evaluate'; params = @{ expression = $js; awaitPromise = $true; returnByValue = $true } } | ConvertTo-Json -Depth 6 -Compress
  $bytes = [Text.Encoding]::UTF8.GetBytes($payload)
  $ws.SendAsync([ArraySegment[byte]]::new($bytes), [System.Net.WebSockets.WebSocketMessageType]::Text, $true, [Threading.CancellationToken]::None).Wait(10000) | Out-Null
  $buffer = New-Object byte[] 262144
  $ms = New-Object System.IO.MemoryStream
  $deadline = [DateTime]::UtcNow.AddMilliseconds($timeoutMs)
  while ($true) {
    if ([DateTime]::UtcNow -gt $deadline) { return @{ error = 'timeout' } }
    $task = $ws.ReceiveAsync([ArraySegment[byte]]::new($buffer), [Threading.CancellationToken]::None)
    while (-not $task.IsCompleted) {
      if ([DateTime]::UtcNow -gt $deadline) { return @{ error = 'timeout' } }
      Start-Sleep -Milliseconds 100
    }
    $rx = $task.Result
    if ($rx.Count -gt 0) { $ms.Write($buffer, 0, $rx.Count) }
    if ($rx.EndOfMessage) {
      $text = [Text.Encoding]::UTF8.GetString($ms.ToArray())
      $ms.SetLength(0)
      $msg = $text | ConvertFrom-Json
      if ($msg.id -eq $myId) { return $msg }
    }
  }
}
function Probe([string]$label, [string]$js) {
  Write-Output "===== $label ====="
  $r = Invoke-Eval $js
  if ($r.error) { Write-Output ("ERROR: " + $r.error); return $null }
  if ($r.result.exceptionDetails) { Write-Output ("JS EXCEPTION: " + ($r.result.exceptionDetails | ConvertTo-Json -Depth 3 -Compress)); return $null }
  Write-Output $r.result.result.value
  return $r.result.result.value
}

# ---------- Phase 0: wait for data (fresh cache rebuild can take minutes) ----------
# The sync badge can read the STALE state file ("synced") while the first
# collection is still running, so wait for actual data (hero > 0), not text.
$syncJs = @'
(() => {
  const el = document.getElementById('syncText');
  const rv = document.querySelector('.ring-value');
  const num = rv ? rv.textContent.replace(/[^\d]/g, '') : '';
  return JSON.stringify({ sync: el ? el.textContent.trim() : '?', ring: num, hero: rv ? rv.textContent.trim() : '(no ring-value)' });
})()
'@
$deadline = [DateTime]::UtcNow.AddSeconds($CollectTimeoutSec)
$sawSyncing = $false
while ($true) {
  $r = Invoke-Eval $syncJs
  $v = $r.result.result.value
  if ($v) {
    if ($v -match '\u540c\u6b65\u4e2d') { $sawSyncing = $true }
    Write-Output ("[poll] " + $v)
    if ($v -match '"ring":"[1-9]/') { break }
  }
  if ([DateTime]::UtcNow -gt $deadline) { Write-Output '[poll] TIMEOUT waiting for data'; break }
  Start-Sleep -Seconds 25
}
Write-Output ("saw syncing state: " + $sawSyncing)

# ---------- Phase A: today ----------
Probe 'A: today page' @'
(async () => {
  const img = document.querySelector('img[src*="logo"]');
  let logoFetch = 'n/a';
  try { const r = await fetch('/shared/logo.png'); logoFetch = r.status + ''; } catch (e) { logoFetch = 'ERR'; }
  const hero = document.querySelector('[class*=today-token], [class*=hero]');
  return JSON.stringify({
    title: document.title,
    logoNatural: img ? (img.naturalWidth + 'x' + img.naturalHeight) : '(none)',
    logoFetch: logoFetch,
    canvasVar: getComputedStyle(document.documentElement).getPropertyValue('--canvas').trim(),
    greenVar: getComputedStyle(document.documentElement).getPropertyValue('--green').trim(),
    hero: hero ? hero.innerText.replace(/\n+/g, ' | ').slice(0, 220) : '(none)',
    bodySample: document.body.innerText.replace(/\n+/g, ' | ').slice(0, 240)
  });
})()
'@

# ---------- Phase B: history ----------
Probe 'B: history tab' @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const cand = [...document.querySelectorAll('button, [role=tab], .tab, [class*=tab], a, div')]
    .filter(e => (e.innerText || '').trim() === '\u5386\u53f2' && e.offsetHeight < 60 && e.querySelectorAll('*').length <= 3 && e.offsetWidth > 0);
  if (!cand.length) return JSON.stringify({ error: 'tab not found' });
  (cand[0].closest('button,[role=tab]') || cand[0]).click();
  await sleep(1500);
  const main = document.querySelector('main') || document.body;
  return JSON.stringify({
    mainEls: main.querySelectorAll('*').length,
    blank: main.innerText.trim().length < 20,
    sample: main.innerText.replace(/\n+/g, ' | ').slice(0, 260)
  });
})()
'@

# ---------- Phase C: privacy ----------
Probe 'C: privacy tab' @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const cand = [...document.querySelectorAll('button, [role=tab], .tab, [class*=tab], a, div')]
    .filter(e => (e.innerText || '').trim() === '\u9690\u79c1' && e.offsetHeight < 60 && e.querySelectorAll('*').length <= 3 && e.offsetWidth > 0);
  if (!cand.length) return JSON.stringify({ error: 'tab not found' });
  (cand[0].closest('button,[role=tab]') || cand[0]).click();
  await sleep(1000);
  const code = [...document.querySelectorAll('code')].map(c => c.textContent);
  return JSON.stringify({
    hasCachePath: code.some(c => c.indexOf('collector-cache.json') >= 0),
    paths: code,
    sample: document.body.innerText.replace(/\n+/g, ' | ').slice(100, 340)
  });
})()
'@

# ---------- Phase D: settings ----------
Probe 'D: settings opens' @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const today = [...document.querySelectorAll('button, [role=tab], .tab, [class*=tab], a, div')]
    .filter(e => (e.innerText || '').trim() === '\u4eca\u65e5' && e.offsetHeight < 60 && e.querySelectorAll('*').length <= 3 && e.offsetWidth > 0);
  if (today.length) (today[0].closest('button,[role=tab]') || today[0]).click();
  await sleep(600);
  const gear = [...document.querySelectorAll('button, .icon-btn, [role=button]')].find(b => /\u8bbe\u7f6e/.test((b.title || '') + (b.getAttribute('aria-label') || '') + (b.innerText || '')));
  if (!gear) return JSON.stringify({ error: 'gear not found' });
  gear.click();
  await sleep(1200);
  const cardTitles = [...document.querySelectorAll('[class*=card-title], h3')].map(e => e.textContent.trim()).filter(Boolean).slice(0, 14);
  const selects = [...document.querySelectorAll('select')].map(s => ({ value: s.value, options: [...s.options].map(o => o.textContent.trim()).slice(0, 8) }));
  const swatches = [...document.querySelectorAll('.theme-swatch')].map(e => ({ theme: e.getAttribute('data-theme'), active: /active|selected|current/.test((e.className || '').toString()) }));
  return JSON.stringify({ cardTitles, selects, swatches });
})()
'@

# ---------- Phase E: real theme switch + restore ----------
Probe 'E: theme switch' @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const read = () => ({
    canvas: getComputedStyle(document.documentElement).getPropertyValue('--canvas').trim(),
    green: getComputedStyle(document.documentElement).getPropertyValue('--green').trim()
  });
  const out = { before: read() };
  const sw = document.querySelector('.theme-swatch[data-theme="ocean"]');
  if (sw) {
    sw.click();
    await sleep(1200);
    out.switched = true;
    out.after = read();
    const restore = document.querySelector('.theme-swatch[data-theme="violet"]');
    if (restore) { restore.click(); await sleep(1000); }
  } else {
    out.switched = false;
    out.after = read();
  }
  if (window.TS && window.TS.applyTheme) { window.TS.applyTheme('violet'); }
  await sleep(500);
  out.restored = read();
  return JSON.stringify(out);
})()
'@

# ---------- Phase F: disk state ----------
Start-Sleep -Seconds 3
$cacheAfter = if (Test-Path $cachePath) { (Get-Item $cachePath).Length } else { 0 }
$proc = Get-Process tokenstep -ErrorAction SilentlyContinue | Select-Object -First 1
Write-Output ("===== F: disk/memory =====")
Write-Output ("cache size after: " + $cacheAfter + " bytes (" + [math]::Round($cacheAfter / 1MB, 1) + " MB)")
if ($proc) { Write-Output ("process working set: " + [math]::Round($proc.WorkingSet64 / 1MB, 0) + " MB") }

$ws.Dispose()
Get-Process tokenstep -ErrorAction SilentlyContinue | Stop-Process -Force
Write-Output 'VERIFICATION DONE'
