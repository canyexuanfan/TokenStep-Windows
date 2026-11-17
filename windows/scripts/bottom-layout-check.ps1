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

# Resize the real app window to 2000x1040 (user's size) before measuring.
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32Win {
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int cmd);
}
"@
Start-Sleep -Seconds 1
$p = Get-Process tokenstep -ErrorAction SilentlyContinue | Select-Object -First 1
if ($p -and $p.MainWindowHandle -ne 0) {
  [Win32Win]::ShowWindow($p.MainWindowHandle, 3) | Out-Null
  Start-Sleep -Seconds 1
  [Win32Win]::ShowWindow($p.MainWindowHandle, 9) | Out-Null
  [Win32Win]::SetWindowPos($p.MainWindowHandle, [IntPtr]::Zero, 0, 0, 2000, 1040, 0x0040) | Out-Null
  Start-Sleep -Seconds 3
  Write-Output "RESIZED"
}
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

# Measure the bottom row live geometry.
$r = Invoke-Eval @'
(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  await sleep(500);
  const rect = (el) => { const r = el.getBoundingClientRect(); return { x: Math.round(r.x), w: Math.round(r.width) }; };
  const grid = document.querySelector(".today-design-bottom");
  const out = {
    window: window.innerWidth,
    grid: grid ? {
      children: grid.children.length,
      columns: getComputedStyle(grid).gridTemplateColumns,
      gridRect: rect(grid),
      kids: [...grid.children].map((c) => ({ cls: String(c.className).slice(0, 40), rect: rect(c), text: (c.textContent || "").replace(/\s+/g, " ").slice(0, 24) }))
    } : null,
    strongWidths: [...document.querySelectorAll(".today-cost-columns strong")].map((s) => { const r = s.getBoundingClientRect(); return { text: s.textContent, w: Math.round(r.width), scrollW: s.scrollWidth, clipped: s.scrollWidth > s.clientWidth + 1 }; })
  };
  return JSON.stringify(out);
})()
'@
Write-Output ("LAYOUT: " + $r)
cmd /c "taskkill /F /T /IM tokenstep.exe" 2>&1 | Out-Null
Write-Output 'LAYOUT-CHECK DONE'
