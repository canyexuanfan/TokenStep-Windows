# End-to-end skin verification: deploy fresh build, launch with CDP, shoot all skins, restore violet.
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
# Kill the whole tree (taskkill /T), not just the main process: Stop-Process
# leaves the WebView2 child tree orphaned, and enough orphans drag WMI down
# until Get-CimInstance — used here previously — hangs for minutes.
cmd /c "taskkill /F /T /IM tokenstep.exe" 2>&1 | Out-Null
Start-Sleep -Seconds 1

if (Test-Path $exeSrc) {
  Copy-Item $exeSrc $exeDst -Force
  Write-Output ("deployed: " + (Get-Item $exeDst).Length + " bytes")
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
if (-not $targets) { Write-Output 'DEBUG PORT NEVER OPENED'; Get-Process tokenstep -ErrorAction SilentlyContinue | Stop-Process -Force; exit 1 }

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

function GotoToday {
  $null = Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const tab = [...document.querySelectorAll('*')].find(e => (e.innerText || '').trim() === '\u4eca\u65e5' && e.offsetHeight < 60 && e.querySelectorAll('*').length <= 3);
  if (tab) (tab.closest('button,[role=tab]') || tab).click();
  await sleep(900);
  return 'today';
})()
'@
}

function ApplySkin([string]$js) {
  $r = Invoke-Eval ("(async () => { const sleep = ms => new Promise(r => setTimeout(r, ms)); " + $js + " await sleep(900); const el = document.documentElement; return JSON.stringify({ canvas: getComputedStyle(el).getPropertyValue('--canvas').trim(), skin: el.getAttribute('data-skin') || '' }); })()")
  Write-Output ("  state: " + $r.result.result.value)
}

ApplySkin "window.TS.applyTheme('violet');"
GotoToday
Shot '1-violet'

ApplySkin "window.TS.applyTheme('voyage','directors_cut');"
GotoToday
Shot '2-directors'

ApplySkin "window.TS.applyTheme('voyage','trojan_inferno');"
GotoToday
Shot '3-trojan'
$null = Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const gear = [...document.querySelectorAll('button, .icon-btn, [role=button]')].find(b => /\u8bbe\u7f6e/.test((b.title || '') + (b.getAttribute('aria-label') || '')));
  if (gear) gear.click();
  await sleep(1000);
  return 'settings';
})()
'@
Shot '3b-settings-trojan'
GotoToday

ApplySkin "window.TS.applyTheme('voyage','ash_marble');"
GotoToday
Shot '4-ash'

ApplySkin "window.TS.applyTheme('event_horizon');"
GotoToday
Shot '5-horizon'
$null = Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const tab = [...document.querySelectorAll('*')].find(e => (e.innerText || '').trim() === '\u5386\u53f2' && e.offsetHeight < 60 && e.querySelectorAll('*').length <= 3);
  if (tab) (tab.closest('button,[role=tab]') || tab).click();
  await sleep(1100);
  return 'history';
})()
'@
Shot '5b-history-horizon'

ApplySkin "window.TS.applyTheme('violet');"
GotoToday

$ws.Dispose()
Get-Process tokenstep -ErrorAction SilentlyContinue | Stop-Process -Force
Write-Output 'E2E-SHOTS DONE'
