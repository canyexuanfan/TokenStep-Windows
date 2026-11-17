# Dump per-source status strings from the live snapshot via CDP.
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
# Wait for first collection pass to complete (poll for non-empty sources).
$deadline0 = [DateTime]::UtcNow.AddSeconds(280)
while ([DateTime]::UtcNow -lt $deadline0) {
  Start-Sleep -Seconds 20
  $probe = Invoke-Eval @'
(async () => {
  const snap = await window.TS.invoke('get_snapshot');
  return JSON.stringify({ n: Object.keys(snap.sources || {}).length, sync: (document.getElementById('syncText')||{textContent:''}).textContent.trim() });
})()
'@
  Write-Output ("PROBE: " + $probe.result.result.value)
  if ($probe.result.result.value -and $probe.result.result.value.Contains('"n":') -and -not $probe.result.result.value.Contains('"n":0')) { break }
}
$ws = [System.Net.WebSockets.ClientWebSocket]::new()
$page = $targets | Where-Object { $_.type -eq 'page' } | Select-Object -First 1
$ws.ConnectAsync([Uri]$page.webSocketDebuggerUrl, [Threading.CancellationToken]::None).Wait(15000)
$payload = @{ id = 1; method = 'Runtime.evaluate'; params = @{ expression = @'
(async () => {
  const snap = await window.TS.invoke('get_snapshot');
  const src = snap.sources || {};
  const out = {};
  Object.keys(src).forEach(k => { out[k] = { status: src[k].status, files: src[k].files, records: src[k].records }; });
  return JSON.stringify(out);
})()
'@; awaitPromise = $true; returnByValue = $true } } | ConvertTo-Json -Depth 6 -Compress
$bytes = [Text.Encoding]::UTF8.GetBytes($payload)
$ws.SendAsync([ArraySegment[byte]]::new($bytes), [System.Net.WebSockets.WebSocketMessageType]::Text, $true, [Threading.CancellationToken]::None).Wait(10000) | Out-Null
$buffer = New-Object byte[] 262144
$ms = New-Object System.IO.MemoryStream
$deadline = [DateTime]::UtcNow.AddSeconds(60)
while ($true) {
  if ([DateTime]::UtcNow -gt $deadline) { Write-Output 'TIMEOUT'; break }
  $task = $ws.ReceiveAsync([ArraySegment[byte]]::new($buffer), [Threading.CancellationToken]::None)
  while (-not $task.IsCompleted) { if ([DateTime]::UtcNow -gt $deadline) { Write-Output 'TIMEOUT'; break }; Start-Sleep -Milliseconds 100 }
  $rx = $task.Result
  if ($rx.Count -gt 0) { $ms.Write($buffer, 0, $rx.Count) }
  if ($rx.EndOfMessage) {
    $msg = [Text.Encoding]::UTF8.GetString($ms.ToArray()) | ConvertFrom-Json
    if ($msg.id -eq 1) { Write-Output ("SOURCES: " + $msg.result.result.value); break }
  }
}
$ws.Dispose()
cmd /c "taskkill /F /T /IM tokenstep.exe" 2>&1 | Out-Null
Write-Output 'STATUS-CHECK DONE'
