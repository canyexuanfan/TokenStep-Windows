# Check the 30-day activity card title (bar chart rename) via CDP.
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

function Invoke-Eval([string]$js, [int]$timeoutMs = 30000) {
  $ws = [System.Net.WebSockets.ClientWebSocket]::new()
  $page = $targets | Where-Object { $_.type -eq 'page' } | Select-Object -First 1
  $ws.ConnectAsync([Uri]$page.webSocketDebuggerUrl, [Threading.CancellationToken]::None).Wait(15000)
  $payload = @{ id = 1; method = 'Runtime.evaluate'; params = @{ expression = $js; returnByValue = $true } } | ConvertTo-Json -Depth 5 -Compress
  $bytes = [Text.Encoding]::UTF8.GetBytes($payload)
  $ws.SendAsync([ArraySegment[byte]]::new($bytes), [System.Net.WebSockets.WebSocketMessageType]::Text, $true, [Threading.CancellationToken]::None).Wait(10000) | Out-Null
  $buffer = New-Object byte[] 131072
  $ms = New-Object System.IO.MemoryStream
  $deadline = [DateTime]::UtcNow.AddMilliseconds($timeoutMs)
  $out = 'NO-REPLY'
  while ([DateTime]::UtcNow -lt $deadline) {
    $task = $ws.ReceiveAsync([ArraySegment[byte]]::new($buffer), [Threading.CancellationToken]::None)
    while (-not $task.IsCompleted) { if ([DateTime]::UtcNow -gt $deadline) { break }; Start-Sleep -Milliseconds 100 }
    $rx = $task.Result
    if ($rx.Count -gt 0) { $ms.Write($buffer, 0, $rx.Count) }
    if ($rx.EndOfMessage) {
      $msg = [Text.Encoding]::UTF8.GetString($ms.ToArray()) | ConvertFrom-Json
      if ($msg.id -eq 1) { $out = $msg.result.result.value; break }
    }
  }
  $ws.Dispose()
  return $out
}

# Wait for sync to finish so the dashboard cards render.
$needle = 'syncText'
$deadline0 = [DateTime]::UtcNow.AddSeconds(240)
while ([DateTime]::UtcNow -lt $deadline0) {
  $txt = Invoke-Eval ('document.getElementById("' + $needle + '") ? document.getElementById("' + $needle + '").textContent.trim() : ""')
  if ($txt -and -not ($txt.Contains([char]21516))) { Write-Output ("SYNC=" + $txt); break }
  Start-Sleep -Seconds 15
}

# \u5929=\u5929 \u6d3b\u52a8=\u6d3b\u52a8 \u6761\u5f62\u56fe=\u6761\u5f62\u56fe \u70ed\u529b\u56fe=\u70ed\u529b\u56fe
$js = 'JSON.stringify({ url: location.pathname, bar: document.body.textContent.includes("30 \u5929\u6d3b\u52a8\u6761\u5f62\u56fe"), heat: document.body.textContent.includes("30 \u5929\u6d3b\u52a8\u70ed\u529b\u56fe"), goalLine: !!document.querySelector(".goal-line") })'
Write-Output ("TITLE: " + (Invoke-Eval $js 45000))
cmd /c "taskkill /F /T /IM tokenstep.exe" 2>&1 | Out-Null
Write-Output 'TITLE-CHECK DONE'
