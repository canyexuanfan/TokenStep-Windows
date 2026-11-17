# Verify the subscription-quota card lists Codex (7d window) on the Today view.
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

function Invoke-Eval([string]$js, [int]$timeoutMs = 60000) {
  $ws = [System.Net.WebSockets.ClientWebSocket]::new()
  $page = $targets | Where-Object { $_.type -eq 'page' } | Select-Object -First 1
  $ws.ConnectAsync([Uri]$page.webSocketDebuggerUrl, [Threading.CancellationToken]::None).Wait(15000)
  $payload = @{ id = 1; method = 'Runtime.evaluate'; params = @{ expression = $js; awaitPromise = $true; returnByValue = $true } } | ConvertTo-Json -Depth 5 -Compress
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

# Wait for sync.
$deadline0 = [DateTime]::UtcNow.AddSeconds(240)
while ([DateTime]::UtcNow -lt $deadline0) {
  $txt = Invoke-Eval 'document.getElementById("syncText") ? document.getElementById("syncText").textContent.trim() : ""'
  if ($txt -and -not ($txt.Contains([char]21516))) { Write-Output ("SYNC=" + $txt); break }
  Start-Sleep -Seconds 15
}

# Force-refresh quota providers and dump the card content.
$r = Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  let providers = null;
  try { providers = await window.TS.invoke('read_quota_providers', { force: true }); } catch (e) { providers = { invokeError: String(e) }; }
  await sleep(1200);
  const card = document.getElementById('quotaProvidersCard');
  const text = card ? card.textContent.replace(/\s+/g, ' ').slice(0, 400) : '(no card)';
  const names = providers && providers.providers ? providers.providers.map(p => p.provider + ':' + p.status).join(',') : JSON.stringify(providers).slice(0, 200);
  return JSON.stringify({ providers: names, card: text });
})()
'@
Write-Output ("QUOTA: " + $r)
cmd /c "taskkill /F /T /IM tokenstep.exe" 2>&1 | Out-Null
Write-Output 'QUOTA-CHECK DONE'
