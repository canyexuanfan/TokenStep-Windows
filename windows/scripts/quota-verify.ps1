# WorkBuddy/Cursor quota providers verification (ASCII-only, CDP).
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
  $payload = @{ id = $sid; method = 'Page.captureScreenshot'; params = @{ format = 'png'; clip = @{ x = $x; y = $y; width = $w; height = $h; scale = 1.5 } } } | ConvertTo-Json -Depth 5 -Compress
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
        $out = "F:\zcode\projects\tokenstep\TokenStep\pictures\quota-$name.png"
        [IO.File]::WriteAllBytes($out, $img)
        Write-Output "CLIP quota-$name.png ($($img.Length) B)"
      }
      return
    }
  }
}

Start-Sleep -Seconds 10
# Open settings, scroll to provider toggles.
$r = Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const gear = [...document.querySelectorAll('button, .icon-btn, [role=button]')].find(b => /\u8bbe\u7f6e/.test((b.title || '') + (b.getAttribute('aria-label') || '')));
  if (gear) gear.click(); await sleep(1000);
  const cb = document.querySelector('input[data-quotaprovider="workbuddy"]');
  if (!cb) return 'no-workbuddy-toggle';
  cb.scrollIntoView({ block: 'center' }); await sleep(700);
  const rect = cb.getBoundingClientRect();
  return JSON.stringify({ wb: 'found', at: Math.round(rect.left) + ',' + Math.round(rect.top), checked: cb.checked });
})()
'@
Write-Output ("PROVIDERS: " + $r.result.result.value)
ClipShot 'provider-toggles' 0 100 1620 760

# Enable WorkBuddy + ensure Cursor on via the UI toggles, then read quota card.
$r = Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const enableIfOff = (id) => { const cb = document.querySelector('input[data-quotaprovider="' + id + '"]'); if (cb && !cb.checked) { cb.click(); return 'turned-on'; } return cb ? 'already-on' : 'missing'; };
  const wbOn = enableIfOff('work_buddy');
  await sleep(1500);
  const curOn = enableIfOff('cursor');
  await sleep(1500);
  const wbNow = document.querySelector('input[data-quotaprovider="workbuddy"]');
  const curNow = document.querySelector('input[data-quotaprovider="cursor"]');
  // scroll to quota card on Today
  const item = document.querySelector('.nav-item[data-id="today"]');
  if (item) item.click();
  await sleep(1200);
  const qc = document.getElementById('quotaProvidersCard') || document.querySelector('[class*=quota]');
  let qrect = null, qtext = '';
  const target = qc || [...document.querySelectorAll('*')].find(e => /(\u8ba2\u9605\u989d\u5ea6|WorkBuddy)/.test(e.textContent || '') && e.offsetHeight > 100 && e.offsetHeight < 700);
  if (target) {
    target.scrollIntoView({ block: 'center' }); await sleep(600);
    const r2 = target.getBoundingClientRect();
    qrect = { x: Math.max(0, Math.round(r2.left)), y: Math.max(0, Math.round(r2.top)), w: Math.min(1620, Math.round(r2.width) + 20), h: Math.min(900, Math.round(r2.height) + 20) };
    qtext = (target.textContent || '').replace(/\s+/g, ' ').slice(0, 200);
  }
  const backend = await window.TS.invoke('read_quota_providers', { force: true });
  const be = {};
  for (const k of Object.keys(backend || {})) {
    const q = backend[k];
    be[k] = q && q.windows ? q.windows.map(w => (w.title || w.kind) + ':' + (w.used_percent != null ? Math.round(w.used_percent) + '%' : '?') + (w.total ? ' ' + Math.round(w.total) : '')) : q;
  }
  return JSON.stringify({ wbOn, curOn, wbChecked: wbNow ? wbNow.checked : null, curChecked: curNow ? curNow.checked : null, backend: be, qrect, qtext });
})()
'@
Write-Output ("QUOTA-INFO: " + $r.result.result.value)
$qinfo = $r.result.result.value | ConvertFrom-Json
if ($qinfo.qrect) {
  ClipShot 'quota-card' $qinfo.qrect.x $qinfo.qrect.y $qinfo.qrect.w $qinfo.qrect.h
}
# Restore WorkBuddy off (user's choice was cursor only), keep Cursor on.
$null = Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const gear = [...document.querySelectorAll('button, .icon-btn, [role=button]')].find(b => /\u8bbe\u7f6e/.test((b.title || '') + (b.getAttribute('aria-label') || '')));
  if (gear) gear.click(); await sleep(800);
  const wbCb = document.querySelector('input[data-quotaprovider="workbuddy"]');
  if (wbCb && wbCb.checked) wbCb.click();
  await sleep(800);
  return 'restored';
})()
'@
$ws.Dispose()
cmd /c "taskkill /F /T /IM tokenstep.exe" 2>&1 | Out-Null
Write-Output 'QUOTA-VERIFY DONE'
