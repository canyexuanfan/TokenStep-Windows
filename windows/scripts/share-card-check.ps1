# Generate all three share cards via CDP and screenshot each canvas.
param([int]$Port = 9223)
$exe = 'E:\Program\TokenStep\tokenstep.exe'
function Test-PortFree([int]$p) {
  $c = New-Object System.Net.Sockets.TcpClient
  try { $c.Connect('127.0.0.1', $p); $c.Close(); return $false } catch { return $true }
}
foreach ($p in 9223, 9224, 9225, 9226) { if (Test-PortFree $p) { $Port = $p; break } }
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port"
Start-Process -FilePath $exe
Start-Sleep -Seconds 4
$targets = $null
for ($i = 0; $i -lt 10; $i++) { try { $targets = Invoke-RestMethod "http://127.0.0.1:$Port/json" -TimeoutSec 2; break } catch { Start-Sleep -Seconds 2 } }
$ws = [System.Net.WebSockets.ClientWebSocket]::new()
$page = $targets | Where-Object { $_.type -eq 'page' } | Select-Object -First 1
$ws.ConnectAsync([Uri]$page.webSocketDebuggerUrl, [Threading.CancellationToken]::None).Wait(15000)
$script:id = 0
function Invoke-Eval([string]$js, [int]$timeoutMs = 90000) {
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
function SaveCanvas([string]$name, [string]$dataUrl) {
  $b64 = $dataUrl -replace '^data:image/png;base64,', ''
  $img = [Convert]::FromBase64String($b64)
  $out = "F:\zcode\projects\tokenstep\TokenStep\pictures\$name.png"
  [IO.File]::WriteAllBytes($out, $img)
  Write-Output "SAVED $name.png ($($img.Length) B)"
}

# Wait for snapshot data.
$deadline = [DateTime]::UtcNow.AddSeconds(300)
$done = $false
while ([DateTime]::UtcNow -lt $deadline -and -not $done) {
  $r = Invoke-Eval @'
(async () => {
  const snap = await window.TS.invoke('get_snapshot');
  return JSON.stringify({ n: (snap.daily || []).length });
})()
'@
  $v = $r.result.result.value | ConvertFrom-Json
  if ($v.n -ge 2) { Write-Output "READY rows=$($v.n)"; $done = $true }
  if (-not $done) { Start-Sleep -Seconds 20 }
}

if ($done) {
  # 1) Daily card (yesterday mode like the user's screenshot).
  $r = Invoke-Eval @'
(async () => {
  const c = document.createElement('canvas');
  c.width = 600;
  const snap = await window.TS.invoke('get_snapshot');
  const settings = await window.TS.invoke('get_settings');
  window.TS.renderShareDailyCard(c, {
    day: snap.daily[snap.daily.length - 2],
    previousDay: snap.daily[snap.daily.length - 3],
    daily30: snap.daily.slice(-30),
    settings: settings, mode: 'yesterday',
    totals: snap.totals || {},
    goalDays: snap.daily.filter(d => d.total_tokens >= settings.daily_goal_tokens).length,
  });
  return c.toDataURL('image/png');
})()
'@
  SaveCanvas 'card-daily' $r.result.result.value

  # 2) Overview card.
  $r2 = Invoke-Eval @'
(async () => {
  const c = document.createElement('canvas');
  c.width = 720;
  const snap = await window.TS.invoke('get_snapshot');
  const settings = await window.TS.invoke('get_settings');
  window.TS.renderTodayOverview(c, { snapshot: snap, settings: settings });
  return c.toDataURL('image/png');
})()
'@
  SaveCanvas 'card-overview' $r2.result.result.value

  # 3) Rhythm card.
  $r3 = Invoke-Eval @'
(async () => {
  const snap = await window.TS.invoke('get_snapshot');
  const yKey = window.TS.yesterdayKey();
  const rhythm = (snap.rhythms || []).filter(r => r.date === yKey && r.total_tokens > 0)[0];
  if (!rhythm) return 'NO-RHYTHM';
  const yDay = (snap.daily || []).filter(d => d.date === yKey)[0] || { total_tokens: rhythm.total_tokens, cost: 0, tools: {}, models: {} };
  const c = document.createElement('canvas');
  c.width = 600; c.height = 840;
  window.TS.renderRhythmCard(c, { rhythm: rhythm, date: rhythm.date, day: yDay });
  return c.toDataURL('image/png');
})()
'@
  if ($r3.result.result.value -ne 'NO-RHYTHM') { SaveCanvas 'card-rhythm' $r3.result.result.value } else { Write-Output 'NO-RHYTHM (skipped)' }
}
$ws.Dispose()
cmd /c "taskkill /F /T /IM tokenstep.exe" 2>&1 | Out-Null
Write-Output 'CARDS DONE'
