# Probe edition line + badges + motion-mode UI under directors pack (ASCII-only).
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
Start-Sleep -Seconds 10
$r = Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  // Direct applyTheme probe: read the stash immediately after the call.
  const direct = await (async () => {
    window.TS.applyTheme('voyage', 'directors_cut');
    const edt0 = document.getElementById('brandEditionText');
    return JSON.stringify({ stash: window.__tsEditionText, text: edt0 ? edt0.textContent : '(no el)', exportsHas: typeof window.TS.syncEditionLine });
  })();
  window.__tsDirectProbe = direct;
  // Real path to directors cut
  const gear = [...document.querySelectorAll('button, .icon-btn, [role=button]')].find(b => /\u8bbe\u7f6e/.test((b.title || '') + (b.getAttribute('aria-label') || '')));
  if (gear) gear.click(); await sleep(800);
  const tab = document.querySelector('.pack-tab[data-pack="odyssey"]'); if (tab) tab.click(); await sleep(300);
  const sw = document.querySelector('.theme-swatch[data-chapter="directors_cut"]'); if (sw) sw.click(); await sleep(1500);
  const ed = document.getElementById('brandEdition');
  const edt = document.getElementById('brandEditionText');
  const cs = ed ? getComputedStyle(ed) : null;
  const ts = edt ? getComputedStyle(edt) : null;
  const out = {
    directProbe: window.__tsDirectProbe,
    editionExists: !!ed,
    display: cs ? cs.display : '(no el)',
    text: edt ? edt.textContent : '(no el)',
    textLen: edt ? edt.textContent.length : -1,
    visibleSize: ed ? (ed.offsetWidth + 'x' + ed.offsetHeight) : '',
    textFont: ts ? (ts.fontSize + '/' + ts.lineHeight + ' color=' + ts.color) : '',
    textSize: edt ? (edt.offsetWidth + 'x' + edt.offsetHeight) : '',
    overflow: cs ? cs.overflow : '',
    parentCls: ed ? (ed.parentElement.className || '').toString().slice(0, 50) : '',
    inHeader: ed ? !!ed.closest('.reference-header') : false
  };
  // motion mode UI present?
  const motionEls = [...document.querySelectorAll('*')].filter(e => /(\u9759\u8c27|\u8f68\u9053|\u5760\u843d)/.test(e.textContent || '') && e.children.length === 0 && e.offsetHeight > 0).slice(0, 4).map(e => ({ tag: e.tagName, text: e.textContent.trim().slice(0, 10), size: e.offsetWidth + 'x' + e.offsetHeight }));
  out.motionModeUI = motionEls;
  // badges: any svg/emblem near theme swatches?
  const badgeCandidates = [...document.querySelectorAll('[data-skin-badge], .skin-badge, .theme-swatch svg')].length;
  out.badgeCandidates = badgeCandidates;
  return JSON.stringify(out);
})()
'@
Write-Output ("EDITION-PROBE: " + $r.result.result.value)
if ($r.result.exceptionDetails) { Write-Output ("EXC: " + ($r.result.exceptionDetails | ConvertTo-Json -Depth 3 -Compress)) }
$ws.Dispose()
cmd /c "taskkill /F /T /IM tokenstep.exe" 2>&1 | Out-Null
Write-Output DONE
