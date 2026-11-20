# Cursor estimation-fix verification: launch new build, wait for collection,
# then check the stats page Cursor / op-5.5 rows and today hero.
# Rules: ASCII-only source (Chinese via \uXXXX), never cancel ReceiveAsync.
param(
  [int]$Port = 9223,
  [string]$Exe = 'E:\Program\TokenStep\tokenstep.exe'
)

function Test-PortFree([int]$p) {
  $c = New-Object System.Net.Sockets.TcpClient
  try { $c.Connect('127.0.0.1', $p); $c.Close(); return $false } catch { return $true }
}
foreach ($p in 9223, 9224, 9225, 9226) { if (Test-PortFree $p) { $Port = $p; break } }
Write-Output "debug port: $Port"

cmd /c "taskkill /F /T /IM tokenstep.exe" 2>&1 | Out-Null
Start-Sleep -Seconds 1

$cachePath = Join-Path $env:APPDATA 'TokenStep\cache\collector-cache.json'
$cacheBefore = if (Test-Path $cachePath) { (Get-Item $cachePath).Length } else { 0 }
Write-Output ("cache size before launch: " + $cacheBefore + " bytes")

$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port"
Start-Process -FilePath $Exe
Start-Sleep -Seconds 3
$targets = $null
for ($i = 0; $i -lt 15; $i++) {
  try { $targets = Invoke-RestMethod "http://127.0.0.1:$Port/json" -TimeoutSec 2; break } catch { Start-Sleep -Seconds 2 }
}
if (-not $targets) { Write-Output 'DEBUG PORT NEVER OPENED'; cmd /c "taskkill /F /T /IM tokenstep.exe" | Out-Null; exit 1 }

$ws = [System.Net.WebSockets.ClientWebSocket]::new()
$page = $targets | Where-Object { $_.type -eq 'page' } | Select-Object -First 1
$ws.ConnectAsync([Uri]$page.webSocketDebuggerUrl, [Threading.CancellationToken]::None).Wait(15000)

$script:id = 0
function Invoke-Eval([string]$js, [int]$timeoutMs = 20000) {
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
    while (-not $task.IsCompleted) {
      if ([DateTime]::UtcNow -gt $deadline) { return @{ error = 'timeout' } }
      Start-Sleep -Milliseconds 100
    }
    $rx = $task.Result
    if ($rx.Count -gt 0) { $ms.Write($buffer, 0, $rx.Count) }
    if ($rx.EndOfMessage) {
      $text = [Text.Encoding]::UTF8.GetString($ms.ToArray())
      $ms.SetLength(0)
      $msg = $text | ConvertFrom-Json
      if ($msg.id -eq $myId) { return $msg }
    }
  }
}
function Probe([string]$label, [string]$js) {
  Write-Output "===== $label ====="
  $r = Invoke-Eval $js
  if ($r.error) { Write-Output ("ERROR: " + $r.error); return $null }
  if ($r.result.exceptionDetails) { Write-Output ("JS EXCEPTION: " + ($r.result.exceptionDetails | ConvertTo-Json -Depth 3 -Compress)); return $null }
  Write-Output $r.result.result.value
  return $r.result.result.value
}

# ---------- Phase 0: wait for first collection ----------
$syncJs = @'
(() => {
  const rv = document.querySelector('.ring-value');
  const errs = window.__lastRenderError || null;
  return JSON.stringify({ ring: rv ? rv.textContent.trim() : '(no ring)', errs: errs });
})()
'@
$deadline = [DateTime]::UtcNow.AddSeconds(300)
while ($true) {
  $r = Invoke-Eval $syncJs
  $v = $r.result.result.value
  if ($v) {
    Write-Output ("[poll] " + $v)
    if ($v -match '"ring":"[1-9]/') { break }
  }
  if ([DateTime]::UtcNow -gt $deadline) { Write-Output '[poll] TIMEOUT waiting for data'; break }
  Start-Sleep -Seconds 20
}

# ---------- Phase A: stats page Cursor + op-5.5 rows ----------
Probe 'A: stats page cursor rows' @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const errs = [];
  window.addEventListener("error", (e) => errs.push(String(e.message).slice(0, 120)));
  const tab = [...document.querySelectorAll("button, a, [role=button], div")]
    .find(b => (b.textContent || "").trim() === '\u7edf\u8ba1' && b.offsetHeight < 60 && b.offsetWidth > 0);
  if (!tab) return JSON.stringify({ error: "stats tab not found" });
  (tab.closest("button,[role=button]") || tab).click();
  await sleep(1200);
  const body = document.body.innerText || "";
  const grab = (re) => {
    const m = body.match(re);
    return m ? m[0].replace(/\n+/g, " ").slice(0, 120) : null;
  };
  return JSON.stringify({
    cursorRow: grab(/Cursor[^\n]{0,80}/g),
    opRow: grab(/op-5\.5[^\n]{0,80}/g),
    errs: errs
  });
})()
'@

# ---------- Phase B: today hero ----------
Probe 'B: today hero' @'
(() => {
  const rv = document.querySelector('.ring-value');
  return JSON.stringify({ ring: rv ? rv.textContent.trim() : null });
})()
'@

$ws.Dispose()
Write-Output 'CHECK DONE (app left running)'
