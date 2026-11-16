# Cursor usage overlay verification: snapshot.tools should include Cursor
# after the refresh pass pulls official events (ASCII-only, CDP).
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
function Invoke-Eval([string]$js, [int]$timeoutMs = 60000) {
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
  $payload = @{ id = $sid; method = 'Page.captureScreenshot'; params = @{ format = 'png'; clip = @{ x = $x; y = $y; width = $w; height = $h; scale = 1 } } } | ConvertTo-Json -Depth 5 -Compress
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
        $out = "F:\zcode\projects\tokenstep\TokenStep\pictures\fix-$name.png"
        [IO.File]::WriteAllBytes($out, $img)
        Write-Output "CLIP fix-$name.png ($($img.Length) B)"
      }
      return
    }
  }
}

# Wait for the first collection+refresh pass (cursor_usage overlay merges).
Start-Sleep -Seconds 12
$deadline = [DateTime]::UtcNow.AddSeconds(300)
$done = $false
while ([DateTime]::UtcNow -lt $deadline -and -not $done) {
  $r = Invoke-Eval @'
(async () => {
  const snap = await window.TS.invoke('get_snapshot');
  const sync = document.getElementById('syncText');
  return JSON.stringify({
    sync: sync ? sync.textContent.trim() : '?',
    tools: snap.tools || [],
    sources: Object.keys(snap.sources || {})
  });
})()
'@
  $v = $r.result.result.value
  Write-Output ("[check] " + $v)
  if ($v -match 'Cursor') { $done = $true }
  if (-not $done) { Start-Sleep -Seconds 20 }
}

if ($done) {
  $r2 = Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const card = [...document.querySelectorAll('.card, section')].find(e => (e.textContent || '').includes('\u6309\u5ba2\u6237\u7aef') && e.offsetHeight > 80 && e.offsetHeight < 600);
  if (card) { card.scrollIntoView({ block: 'center' }); }
  await sleep(600);
  const rect = card ? card.getBoundingClientRect() : null;
  return JSON.stringify({ rect: rect ? { x: Math.max(0, Math.round(rect.left)), y: Math.max(0, Math.round(rect.top)), w: Math.round(rect.width), h: Math.round(rect.height) } : null, sample: card ? card.textContent.replace(/\s+/g, ' ').slice(0, 260) : '' });
})()
'@
  Write-Output ("CARD: " + $r2.result.result.value)
  $info = $r2.result.result.value | ConvertFrom-Json
  if ($info.rect -and $info.rect.h -gt 60) { ClipShot 'per-client' $info.rect.x $info.rect.y ([Math]::Max(400, $info.rect.w)) ([Math]::Max(120, $info.rect.h)) }
}
$ws.Dispose()
cmd /c "taskkill /F /T /IM tokenstep.exe" 2>&1 | Out-Null
Write-Output 'OVERLAY-VERIFY DONE'
