# Verify skin plates actually load and paint (ASCII-only, CDP).
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
Start-Sleep -Seconds 3
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

# 1) raw fetch of all five plates
$r = Invoke-Eval @'
(async () => {
  const urls = [
    '/shared/skins/odyssey/OdysseyAegeanPopover.png',
    '/shared/skins/odyssey/OdysseyTrojanPopover.png',
    '/shared/skins/odyssey/OdysseyAshMarblePopover.png',
    '/shared/skins/interstellar/InterstellarEventHorizonHero.png',
    '/shared/skins/interstellar/InterstellarEventHorizonQuiet.png'
  ];
  const out = {};
  for (const u of urls) {
    try { const r = await fetch(u); out[u.split('/').pop()] = r.status + ' ' + (r.headers.get('content-length') || '?'); }
    catch (e) { out[u.split('/').pop()] = 'ERR'; }
  }
  return JSON.stringify(out);
})()
'@
Write-Output ("PLATES-FETCH: " + $r.result.result.value)

# 2) apply trojan inferno, read hero card computed background-image
$r = Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  window.TS.applyTheme('voyage', 'trojan_inferno');
  await sleep(600);
  const hero = document.querySelector('#view .today-hero-slot > .card:first-child');
  const bi = hero ? getComputedStyle(hero).backgroundImage : '(no hero)';
  return JSON.stringify({ heroBI: bi.slice(0, 220) });
})()
'@
Write-Output ("HERO-BG-TROJAN: " + $r.result.result.value)

# 3) decode-check: draw the plate into a canvas and read a pixel (proves real image bytes)
$r = Invoke-Eval @'
(async () => {
  const img = new Image();
  const loaded = new Promise((res, rej) => { img.onload = res; img.onerror = rej; });
  img.src = '/shared/skins/odyssey/OdysseyTrojanPopover.png';
  try { await loaded; } catch (e) { return JSON.stringify({ decode: 'FAILED' }); }
  const c = document.createElement('canvas');
  c.width = 16; c.height = 16;
  const ctx = c.getContext('2d');
  ctx.drawImage(img, 0, 0, 16, 16);
  const d = ctx.getImageData(0, 0, 1, 1).data;
  return JSON.stringify({ decode: 'OK', natural: img.naturalWidth + 'x' + img.naturalHeight, pixel: [d[0], d[1], d[2], d[3]] });
})()
'@
Write-Output ("PLATE-DECODE: " + $r.result.result.value)

# 4) back to violet: art var must be cleared
$r = Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  window.TS.applyTheme('violet');
  await sleep(400);
  return getComputedStyle(document.documentElement).getPropertyValue('--skin-art').trim() || '(cleared)';
})()
'@
Write-Output ("VIOLET-ART-VAR: " + $r.result.result.value)

$ws.Dispose()
Get-Process tokenstep -ErrorAction SilentlyContinue | Stop-Process -Force
Write-Output 'PLATE-VERIFY DONE'
