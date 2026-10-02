# Forensic probe: edition line live values + clipped close-ups for human review.
param([int]$Port = 9223)
$exe = 'E:\Program\TokenStep\tokenstep.exe'
function Test-PortFree([int]$p) {
  $c = New-Object System.Net.Sockets.TcpClient
  try { $c.Connect('127.0.0.1', $p); $c.Close(); return $false } catch { return $true }
}
foreach ($p in 9223, 9224, 9225, 9226) { if (Test-PortFree $p) { $Port = $p; break } }
cmd /c "taskkill /F /T /IM tokenstep.exe" 2>&1 | Out-Null
Start-Sleep -Seconds 1
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port"
Start-Process -FilePath $exe
Start-Sleep -Seconds 3
$targets = $null
for ($i = 0; $i -lt 12; $i++) { try { $targets = Invoke-RestMethod "http://127.0.0.1:$Port/json" -TimeoutSec 2; break } catch { Start-Sleep -Seconds 2 } }
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
function ClipShot([string]$name, [int]$x, [int]$y, [int]$w, [int]$h) {
  $script:id++
  $sid = 9800 + $script:id
  $payload = @{ id = $sid; method = 'Page.captureScreenshot'; params = @{ format = 'png'; clip = @{ x = $x; y = $y; width = $w; height = $h; scale = 2 } } } | ConvertTo-Json -Depth 5 -Compress
  $bytes = [Text.Encoding]::UTF8.GetBytes($payload)
  $ws.SendAsync([ArraySegment[byte]]::new($bytes), [System.Net.WebSockets.WebSocketMessageType]::Text, $true, [Threading.CancellationToken]::None).Wait(10000) | Out-Null
  $buffer = New-Object byte[] 524288
  $ms = New-Object System.IO.MemoryStream
  $deadline = [DateTime]::UtcNow.AddSeconds(15)
  while ($true) {
    if ([DateTime]::UtcNow -gt $deadline) { Write-Output "CLIP TIMEOUT $name"; return }
    $task = $ws.ReceiveAsync([ArraySegment[byte]]::new($buffer), [Threading.CancellationToken]::None)
    while (-not $task.IsCompleted) { if ([DateTime]::UtcNow -gt $deadline) { Write-Output "CLIP TIMEOUT $name"; return }; Start-Sleep -Milliseconds 50 }
    $rx = $task.Result
    if ($rx.Count -gt 0) { $ms.Write($buffer, 0, $rx.Count) }
    if ($rx.EndOfMessage) {
      $msg = [Text.Encoding]::UTF8.GetString($ms.ToArray()) | ConvertFrom-Json
      if ($msg.id -eq $sid -and $msg.result.data) {
        $img = [Convert]::FromBase64String($msg.result.data)
        $out = "F:\zcode\projects\tokenstep\TokenStep\pictures\forensic-$name.png"
        [IO.File]::WriteAllBytes($out, $img)
        Write-Output "CLIP forensic-$name.png ($($img.Length) B)"
      }
      return
    }
  }
}

function ResetViewTop {
  $null = Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  window.scrollTo(0, 0);
  const sc = document.querySelector('main.content');
  if (sc) sc.scrollTop = 0;
  if (document.scrollingElement) document.scrollingElement.scrollTop = 0;
  await sleep(1200);
  return 'top';
})()
'@
}

Start-Sleep -Seconds 10
# 1) Directors cut via real path, then read edition line live values.
$r = Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const gear = [...document.querySelectorAll('button, .icon-btn, [role=button]')].find(b => /\u8bbe\u7f6e/.test((b.title || '') + (b.getAttribute('aria-label') || '')));
  if (gear) gear.click(); await sleep(800);
  const tab = document.querySelector('.pack-tab[data-pack="odyssey"]'); if (tab) tab.click(); await sleep(300);
  const sw = document.querySelector('.theme-swatch[data-chapter="directors_cut"]'); if (sw) sw.click(); await sleep(1500);
  const edt = document.getElementById('brandEditionText');
  const ed = document.getElementById('brandEdition');
  const rect = ed ? ed.getBoundingClientRect() : null;
  const cs = ed ? getComputedStyle(ed) : null;
  const ts = edt ? getComputedStyle(edt) : null;
  return JSON.stringify({
    stash: window.__tsEditionText,
    text: edt ? edt.textContent : '(no el)',
    textLen: edt ? edt.textContent.length : -1,
    display: cs ? cs.display : '',
    size: ed ? ed.offsetWidth + 'x' + ed.offsetHeight : '',
    pos: rect ? Math.round(rect.left) + ',' + Math.round(rect.top) : '',
    font: ts ? ts.fontSize : '',
    color: ts ? ts.color : ''
  });
})()
'@
Write-Output ("EDITION-LIVE: " + $r.result.result.value)

# 2) Close-up of the header area (edition line zone).
ResetViewTop
ClipShot 'header-directors' 0 0 900 130

# 3) Scroll the motion-mode buttons into view and clip that zone.
$r = Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const btn = [...document.querySelectorAll('button')].find(b => (b.textContent || '').trim() === '\u9759\u8c27');
  if (!btn) return 'no-btn';
  btn.scrollIntoView({ block: 'center' });
  await sleep(600);
  const rect = btn.getBoundingClientRect();
  return JSON.stringify({ btnAt: Math.round(rect.left) + ',' + Math.round(rect.top), btnSize: Math.round(rect.width) + 'x' + Math.round(rect.height) });
})()
'@
Write-Output ("SCROLL: " + $r.result.result.value)
ClipShot 'motion-modes' 0 150 1620 700

# 4) Horizon edition text check.
$r = Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const tab = document.querySelector('.pack-tab[data-pack="interstellar"]'); if (tab) tab.click(); await sleep(300);
  const sw = document.querySelector('.theme-swatch[data-theme="event_horizon"]'); if (sw) sw.click(); await sleep(1500);
  const edt = document.getElementById('brandEditionText');
  return JSON.stringify({ text: edt ? edt.textContent : '(no el)', len: edt ? edt.textContent.length : -1 });
})()
'@
Write-Output ("HORIZON-EDITION: " + $r.result.result.value)
ResetViewTop
Start-Sleep -Milliseconds 800
ClipShot 'header-horizon' 0 0 900 130

# Restore violet via real path.
$r = Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const tab = document.querySelector('.pack-tab[data-pack="classic"]'); if (tab) tab.click(); await sleep(300);
  const sw = document.querySelector('.theme-swatch[data-theme="violet"]'); if (sw) sw.click(); await sleep(1200);
  return getComputedStyle(document.documentElement).getPropertyValue('--canvas').trim();
})()
'@
Write-Output ("RESTORED: " + $r.result.result.value)

$ws.Dispose()
cmd /c "taskkill /F /T /IM tokenstep.exe" 2>&1 | Out-Null
Write-Output 'FORENSIC DONE'
