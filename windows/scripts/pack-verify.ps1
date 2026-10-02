# Skin-pack switching verification + screenshots (ASCII-only, CDP).
param([int]$Port = 9223)

$exe = 'E:\Program\TokenStep\tokenstep.exe'
function Test-PortFree([int]$p) {
  $c = New-Object System.Net.Sockets.TcpClient
  try { $c.Connect('127.0.0.1', $p); $c.Close(); return $false } catch { return $true }
}
foreach ($p in 9223, 9224, 9225, 9226) { if (Test-PortFree $p) { $Port = $p; break } }
Write-Output "debug port: $Port"

Get-Process tokenstep -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1
# Whole-tree kill: Stop-Process orphans the WebView2 child tree.
cmd /c "taskkill /F /T /IM tokenstep.exe" 2>&1 | Out-Null
Start-Sleep -Seconds 1

$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port"
Start-Process -FilePath $exe
Start-Sleep -Seconds 3
$targets = $null
for ($i = 0; $i -lt 15; $i++) {
  try { $targets = Invoke-RestMethod "http://127.0.0.1:$Port/json" -TimeoutSec 2; break } catch { Start-Sleep -Seconds 2 }
}
if (-not $targets) { Write-Output 'DEBUG PORT NEVER OPENED'; exit 1 }

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

function Shot([string]$name) {
  $script:id++
  $shotId = 9000 + $script:id
  $payload = @{ id = $shotId; method = 'Page.captureScreenshot'; params = @{ format = 'png' } } | ConvertTo-Json -Depth 4 -Compress
  $bytes = [Text.Encoding]::UTF8.GetBytes($payload)
  $ws.SendAsync([ArraySegment[byte]]::new($bytes), [System.Net.WebSockets.WebSocketMessageType]::Text, $true, [Threading.CancellationToken]::None).Wait(10000) | Out-Null
  $buffer = New-Object byte[] 262144
  $ms = New-Object System.IO.MemoryStream
  $deadline = [DateTime]::UtcNow.AddSeconds(15)
  while ($true) {
    if ([DateTime]::UtcNow -gt $deadline) { Write-Output "SHOT TIMEOUT $name"; return }
    $task = $ws.ReceiveAsync([ArraySegment[byte]]::new($buffer), [Threading.CancellationToken]::None)
    while (-not $task.IsCompleted) {
      if ([DateTime]::UtcNow -gt $deadline) { Write-Output "SHOT TIMEOUT $name"; return }
      Start-Sleep -Milliseconds 50
    }
    $rx = $task.Result
    if ($rx.Count -gt 0) { $ms.Write($buffer, 0, $rx.Count) }
    if ($rx.EndOfMessage) {
      $msg = [Text.Encoding]::UTF8.GetString($ms.ToArray()) | ConvertFrom-Json
      if ($msg.id -eq $shotId -and $msg.result.data) {
        $img = [Convert]::FromBase64String($msg.result.data)
        $out = "F:\zcode\projects\tokenstep\TokenStep\pictures\pack-$name.png"
        [IO.File]::WriteAllBytes($out, $img)
        Write-Output "SHOT $out ($($img.Length) B)"
      }
      return
    }
  }
}

# Open settings (gear), return today first.
$r = Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const gear = [...document.querySelectorAll('button, .icon-btn, [role=button]')].find(b => /\u8bbe\u7f6e/.test((b.title || '') + (b.getAttribute('aria-label') || '')));
  if (!gear) return JSON.stringify({ error: 'no gear' });
  gear.click();
  await sleep(1000);
  const tabs = [...document.querySelectorAll('.pack-tab')].map(t => t.getAttribute('data-pack'));
  const groups = [...document.querySelectorAll('.pack-options')].map(g => g.id + ':' + g.style.display);
  return JSON.stringify({ tabs, groups });
})()
'@
Write-Output ("OPEN-SETTINGS: " + $r.result.result.value)

# Switch sequences: click pack tab then a swatch inside it; verify vars.
function Switch-And-Verify([string]$label, [string]$js) {
  $r = Invoke-Eval $js
  Write-Output ("$label -> " + $r.result.result.value)
  Start-Sleep -Milliseconds 800
  Shot $label
}

Switch-And-Verify '1-violet' @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const tab = [...document.querySelectorAll('.pack-tab')].find(t => t.getAttribute('data-pack') === 'classic');
  tab.click(); await sleep(300);
  const sw = document.querySelector('.theme-swatch[data-theme="violet"]');
  sw.click(); await sleep(1200);
  return JSON.stringify({
    canvas: getComputedStyle(document.documentElement).getPropertyValue('--canvas').trim(),
    green: getComputedStyle(document.documentElement).getPropertyValue('--green').trim()
  });
})()
'@

Switch-And-Verify '2-odyssey-directors' @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const tab = [...document.querySelectorAll('.pack-tab')].find(t => t.getAttribute('data-pack') === 'odyssey');
  tab.click(); await sleep(300);
  const sw = document.querySelector('.theme-swatch[data-chapter="directors_cut"]');
  sw.click(); await sleep(1200);
  return JSON.stringify({
    canvas: getComputedStyle(document.documentElement).getPropertyValue('--canvas').trim(),
    green: getComputedStyle(document.documentElement).getPropertyValue('--green').trim()
  });
})()
'@

Switch-And-Verify '3-trojan-inferno' @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const sw = document.querySelector('.theme-swatch[data-chapter="trojan_inferno"]');
  sw.click(); await sleep(1200);
  return JSON.stringify({
    canvas: getComputedStyle(document.documentElement).getPropertyValue('--canvas').trim(),
    green: getComputedStyle(document.documentElement).getPropertyValue('--green').trim()
  });
})()
'@

# Back to today view under trojan inferno for a real-content shot.
Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const tab = [...document.querySelectorAll('*')].find(e => (e.innerText || '').trim() === '\u4eca\u65e5' && e.offsetHeight < 60 && e.querySelectorAll('*').length <= 3);
  if (tab) (tab.closest('button,[role=tab]') || tab).click();
  await sleep(1000);
  return 'today';
})()
'@
Shot '3b-today-trojan'

Switch-And-Verify '4-event-horizon' @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const gear = [...document.querySelectorAll('button, .icon-btn, [role=button]')].find(b => /\u8bbe\u7f6e/.test((b.title || '') + (b.getAttribute('aria-label') || '')));
  gear.click(); await sleep(800);
  const tab = [...document.querySelectorAll('.pack-tab')].find(t => t.getAttribute('data-pack') === 'interstellar');
  tab.click(); await sleep(300);
  const sw = document.querySelector('.theme-swatch[data-theme="event_horizon"]');
  sw.click(); await sleep(1200);
  return JSON.stringify({
    canvas: getComputedStyle(document.documentElement).getPropertyValue('--canvas').trim(),
    green: getComputedStyle(document.documentElement).getPropertyValue('--green').trim()
  });
})()
'@

Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const tab = [...document.querySelectorAll('*')].find(e => (e.innerText || '').trim() === '\u4eca\u65e5' && e.offsetHeight < 60 && e.querySelectorAll('*').length <= 3);
  if (tab) (tab.closest('button,[role=tab]') || tab).click();
  await sleep(1000);
  return 'today';
})()
'@
Shot '4b-today-horizon'

# Restore violet (user's choice) and persist.
$r = Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const gear = [...document.querySelectorAll('button, .icon-btn, [role=button]')].find(b => /\u8bbe\u7f6e/.test((b.title || '') + (b.getAttribute('aria-label') || '')));
  gear.click(); await sleep(800);
  const tab = [...document.querySelectorAll('.pack-tab')].find(t => t.getAttribute('data-pack') === 'classic');
  tab.click(); await sleep(300);
  const sw = document.querySelector('.theme-swatch[data-theme="violet"]');
  sw.click(); await sleep(1200);
  return getComputedStyle(document.documentElement).getPropertyValue('--canvas').trim();
})()
'@
Write-Output ("RESTORED-VIOLET: " + $r.result.result.value)

$ws.Dispose()
Get-Process tokenstep -ErrorAction SilentlyContinue | Stop-Process -Force
Write-Output 'PACK-VERIFY DONE'
