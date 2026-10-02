# Quick DOM probe for hero card under skin theme (ASCII-only).
param([int]$Port = 9223)
$exe = 'E:\Program\TokenStep\tokenstep.exe'
function Test-PortFree([int]$p) {
  $c = New-Object System.Net.Sockets.TcpClient
  try { $c.Connect('127.0.0.1', $p); $c.Close(); return $false } catch { return $true }
}
foreach ($p in 9223, 9224, 9225, 9226) { if (Test-PortFree $p) { $Port = $p; break } }
Get-Process tokenstep -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1
# Whole-tree kill: Stop-Process orphans the WebView2 child tree.
cmd /c "taskkill /F /T /IM tokenstep.exe" 2>&1 | Out-Null
Start-Sleep -Seconds 1
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port"
Start-Process -FilePath $exe
Start-Sleep -Seconds 12
$targets = $null
for ($i = 0; $i -lt 15; $i++) { try { $targets = Invoke-RestMethod "http://127.0.0.1:$Port/json" -TimeoutSec 2; break } catch { Start-Sleep -Seconds 2 } }
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
$r = Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  await sleep(2500);
  const out = {};
  out.viewExists = !!document.getElementById('view');
  const slot = document.querySelector('.today-hero-slot');
  out.slotExists = !!slot;
  if (slot) {
    out.slotChildren = [...slot.children].map(c => c.className || c.tagName).slice(0, 4);
    const card = slot.querySelector('.card');
    out.cardFound = !!card;
    if (card) {
      const cs = getComputedStyle(card);
      out.bgImage = cs.backgroundImage.slice(0, 260);
      out.size = card.offsetWidth + 'x' + card.offsetHeight;
    }
  } else {
    out.viewHTMLHead = (document.getElementById('view') || {}).innerHTML?.slice(0, 120);
  }
  window.TS.applyTheme('voyage', 'trojan_inferno');
  await sleep(500);
  const slot2 = document.querySelector('.today-hero-slot');
  if (slot2) {
    const card2 = slot2.querySelector('.card');
    if (card2) out.bgImageTrojan = getComputedStyle(card2).backgroundImage.slice(0, 300);
  }
  return JSON.stringify(out);
})()
'@
Write-Output ("PROBE: " + $r.result.result.value)
if ($r.result.exceptionDetails) { Write-Output ("EXC: " + ($r.result.exceptionDetails | ConvertTo-Json -Depth 3 -Compress)) }
$ws.Dispose()
Get-Process tokenstep -ErrorAction SilentlyContinue | Stop-Process -Force
Write-Output DONE
