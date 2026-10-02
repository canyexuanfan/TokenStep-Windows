# End-to-end skin verification: deploy fresh build, launch with CDP, drive the
# REAL settings path to switch skins (so SVG-inline colors re-render), shoot
# all skins with motion frozen, restore the user's persisted choice.
param([int]$Port = 9223)
$ErrorActionPreference = 'Continue'
$repo = 'F:\zcode\projects\tokenstep\TokenStep'
$exeSrc = Join-Path $repo 'windows\src-tauri\target\release\tokenstep.exe'
$exeDst = 'E:\Program\TokenStep\tokenstep.exe'

function Test-PortFree([int]$p) {
  $c = New-Object System.Net.Sockets.TcpClient
  try { $c.Connect('127.0.0.1', $p); $c.Close(); return $false } catch { return $true }
}
foreach ($p in 9223, 9224, 9225, 9226) { if (Test-PortFree $p) { $Port = $p; break } }
Write-Output "debug port: $Port"

Get-Process tokenstep -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1
# Whole-tree kill (taskkill /T): Stop-Process orphans the WebView2 child tree,
# and enough orphans drag WMI/desktop-heap down with them.
cmd /c "taskkill /F /T /IM tokenstep.exe" 2>&1 | Out-Null
Start-Sleep -Seconds 1

if (Test-Path $exeSrc) {
  Copy-Item $exeSrc $exeDst -Force
  Start-Sleep -Milliseconds 800
  $srcLen = (Get-Item $exeSrc).Length
  $dstLen = (Get-Item $exeDst).Length
  if ($srcLen -ne $dstLen) { Write-Output "DEPLOY MISMATCH src=$srcLen dst=$dstLen"; cmd /c "taskkill /F /T /IM tokenstep.exe" 2>&1 | Out-Null; exit 1 }
  Write-Output ("deployed: " + $dstLen + " bytes")
} else {
  Write-Output "NO BUILD AT $exeSrc"
  exit 1
}

$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port"
Start-Process -FilePath $exeDst
Start-Sleep -Seconds 3
$targets = $null
for ($i = 0; $i -lt 15; $i++) {
  try { $targets = Invoke-RestMethod "http://127.0.0.1:$Port/json" -TimeoutSec 2; break } catch { Start-Sleep -Seconds 2 }
}
if (-not $targets) { Write-Output 'DEBUG PORT NEVER OPENED'; cmd /c "taskkill /F /T /IM tokenstep.exe" 2>&1 | Out-Null; exit 1 }

$ws = [System.Net.WebSockets.ClientWebSocket]::new()
$page = $targets | Where-Object { $_.type -eq 'page' } | Select-Object -First 1
$ws.ConnectAsync([Uri]$page.webSocketDebuggerUrl, [Threading.CancellationToken]::None).Wait(15000)

$script:id = 0
function Invoke-Eval([string]$js, [int]$timeoutMs = 30000) {
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
    while (-not $task.IsCompleted) { if ([DateTime]::UtcNow -gt $deadline) { return @{ error = 'timeout' } }; Start-Sleep -Milliseconds 100 }
    $rx = $task.Result
    if ($rx.Count -gt 0) { $ms.Write($buffer, 0, $rx.Count) }
    if ($rx.EndOfMessage) {
      $msg = [Text.Encoding]::UTF8.GetString($ms.ToArray()) | ConvertFrom-Json
      if ($msg.id -eq $myId) { return $msg }
    }
  }
}
function Shot([string]$name) {
  $script:id++
  $shotId = 9700 + $script:id
  $payload = @{ id = $shotId; method = 'Page.captureScreenshot'; params = @{ format = 'png' } } | ConvertTo-Json -Depth 4 -Compress
  $bytes = [Text.Encoding]::UTF8.GetBytes($payload)
  $ws.SendAsync([ArraySegment[byte]]::new($bytes), [System.Net.WebSockets.WebSocketMessageType]::Text, $true, [Threading.CancellationToken]::None).Wait(10000) | Out-Null
  $buffer = New-Object byte[] 262144
  $ms = New-Object System.IO.MemoryStream
  $deadline = [DateTime]::UtcNow.AddSeconds(15)
  while ($true) {
    if ([DateTime]::UtcNow -gt $deadline) { Write-Output "SHOT TIMEOUT $name"; return }
    $task = $ws.ReceiveAsync([ArraySegment[byte]]::new($buffer), [Threading.CancellationToken]::None)
    while (-not $task.IsCompleted) { if ([DateTime]::UtcNow -gt $deadline) { Write-Output "SHOT TIMEOUT $name"; return }; Start-Sleep -Milliseconds 50 }
    $rx = $task.Result
    if ($rx.Count -gt 0) { $ms.Write($buffer, 0, $rx.Count) }
    if ($rx.EndOfMessage) {
      $msg = [Text.Encoding]::UTF8.GetString($ms.ToArray()) | ConvertFrom-Json
      if ($msg.id -eq $shotId -and $msg.result.data) {
        $img = [Convert]::FromBase64String($msg.result.data)
        $out = "F:\zcode\projects\tokenstep\TokenStep\pictures\e2e-$name.png"
        [IO.File]::WriteAllBytes($out, $img)
        Write-Output "SHOT e2e-$name.png ($($img.Length) B)"
      }
      return
    }
  }
}

# Wait for app data to render.
Start-Sleep -Seconds 12

# Ambient-motion probe + deterministic-shot freeze (upstream rule). The pause
# class is re-added after every skin switch below (applyTheme re-judges it).
$fx = Invoke-Eval @'
(() => {
  document.documentElement.classList.add('motion-paused');
  const glow = document.getElementById('skinFxGlow');
  const embers = document.getElementById('skinFxEmbers');
  const gs = glow ? getComputedStyle(glow) : null;
  const es = embers ? getComputedStyle(embers) : null;
  return JSON.stringify({
    glowLayer: !!glow,
    glowAnim: gs ? gs.animationName : '(no layer)',
    emberLayer: !!embers,
    emberAnim: es ? es.animationName : '(no layer)'
  });
})()
'@
Write-Output ("FX-CHECK: " + $fx.result.result.value)

function GotoToday {
  $null = Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const item = document.querySelector('.nav-item[data-id="today"]');
  if (item) item.click();
  await sleep(700);
  // Browsers restore the last scroll position on nav; every shot needs the
  // page top (header + hero) in frame.
  window.scrollTo(0, 0);
  const sc = document.querySelector('main.content');
  if (sc) sc.scrollTop = 0;
  if (document.scrollingElement) document.scrollingElement.scrollTop = 0;
  await sleep(400);
  return 'today:' + (document.querySelector('#view .today-token-card') ? 'ok' : 'missing');
})()
'@
}

function GotoHistory {
  $null = Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const item = document.querySelector('.nav-item[data-id="history"]');
  if (item) item.click();
  await sleep(1100);
  return 'history';
})()
'@
}

# Drive the REAL settings path: gear -> pack tab -> swatch click. The app's
# own swatch handler runs invoke('set_theme'/'set_odyssey_chapter') and then
# render(), so SVG-inline colors (ring track, rhythm chip) follow the theme.
# In-memory applyTheme only re-skins CSS vars and leaves those baked from the
# startup theme — the bug this replaces.
function SwitchSkin([string]$pack, [string]$swatchSelector, [string]$label) {
  # PS single-quoted literal: '' inside = one '. Placeholders replaced after.
  $js = '(async () => { const sleep = ms => new Promise(r => setTimeout(r, ms)); const gear = [...document.querySelectorAll(''button, .icon-btn, [role=button]'')].find(b => /\u8bbe\u7f6e/.test((b.title || '''') + (b.getAttribute(''aria-label'') || ''''))); if (gear) gear.click(); await sleep(900); const tab = document.querySelector(''.pack-tab[data-pack="PACK"]''); if (tab) tab.click(); await sleep(400); const sw = document.querySelector(''SWATCH''); if (sw) sw.click(); await sleep(1500); document.documentElement.classList.add(''motion-paused''); return JSON.stringify({ want: ''LABEL'', canvas: getComputedStyle(document.documentElement).getPropertyValue(''--canvas'').trim(), pack: document.documentElement.hasAttribute(''data-skin-pack'') }); })()'
  $js = $js.Replace('PACK', $pack).Replace('SWATCH', $swatchSelector).Replace('LABEL', $label)
  $r = Invoke-Eval $js
  Write-Output ("  state: " + $r.result.result.value)
}

# Persist violet FIRST through the real path so the run starts from a known,
# natively-rendered baseline (settings.json holds whatever was last clicked).
SwitchSkin 'classic' '.theme-swatch[data-theme="violet"]' 'violet'
GotoToday
Shot '1-violet'

SwitchSkin 'odyssey' '.theme-swatch[data-chapter="directors_cut"]' 'directors'
GotoToday
Shot '2-directors'

SwitchSkin 'odyssey' '.theme-swatch[data-chapter="trojan_inferno"]' 'trojan'
GotoToday
Shot '3-trojan'

# Settings page under trojan: open settings (already on trojan).
$null = Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const gear = [...document.querySelectorAll('button, .icon-btn, [role=button]')].find(b => /\u8bbe\u7f6e/.test((b.title || '') + (b.getAttribute('aria-label') || '')));
  if (gear) gear.click();
  await sleep(1000);
  return 'settings';
})()
'@
# Scroll the settings page to the theme pack card so the motion-mode picker
# and chapter badges are in frame (3b keeps the top view).
$null = Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const scroller = document.querySelector('main.content') || document.scrollingElement;
  const btn = [...document.querySelectorAll('button')].find(b => (b.textContent || '').trim() === '静谧');
  if (btn && btn.scrollIntoView) { btn.scrollIntoView({ block: 'center' }); }
  await sleep(700);
  return 'scrolled:' + (btn ? 'found-btn' : 'no-btn');
})()
'@
Shot '3c-settings-theme-card'
Shot '3b-settings-trojan'

SwitchSkin 'odyssey' '.theme-swatch[data-chapter="ash_marble"]' 'ash'
GotoToday
Shot '4-ash'

SwitchSkin 'interstellar' '.theme-swatch[data-theme="event_horizon"]' 'horizon'
GotoToday
Shot '5-horizon'
GotoHistory
Shot '5b-history-horizon'

# Leave the app on the user's own persisted classic choice.
SwitchSkin 'classic' '.theme-swatch[data-theme="violet"]' 'restore-violet'
GotoToday

$ws.Dispose()
cmd /c "taskkill /F /T /IM tokenstep.exe" 2>&1 | Out-Null
Write-Output 'E2E-SHOTS DONE'
