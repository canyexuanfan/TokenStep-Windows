# Screenshot the fixed bottom row (cost + privacy cards) at 2000px window.
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

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32Shot {
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int cmd);
}
"@
Start-Sleep -Seconds 1
$p = Get-Process tokenstep -ErrorAction SilentlyContinue | Select-Object -First 1
if ($p -and $p.MainWindowHandle -ne 0) {
  [Win32Shot]::ShowWindow($p.MainWindowHandle, 9) | Out-Null
  [Win32Shot]::SetWindowPos($p.MainWindowHandle, [IntPtr]::Zero, 0, 0, 2000, 1040, 0x0040) | Out-Null
  Start-Sleep -Seconds 3
  Write-Output "RESIZED"
}

$ws = [System.Net.WebSockets.ClientWebSocket]::new()
$page = $targets | Where-Object { $_.type -eq 'page' } | Select-Object -First 1
$ws.ConnectAsync([Uri]$page.webSocketDebuggerUrl, [Threading.CancellationToken]::None).Wait(15000)
$script:id = 0
function Invoke-Eval([string]$js, [int]$timeoutMs = 60000) {
  $script:id++
  $myId = $script:id
  $payload = @{ id = $myId; method = 'Runtime.evaluate'; params = @{ expression = $js; awaitPromise = $true; returnByValue = $true } } | ConvertTo-Json -Depth 5 -Compress
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
      if ($msg.id -eq $myId) { $out = $msg.result.result.value; break }
    }
  }
  return $out
}
function ClipShot([string]$name, [int]$x, [int]$y, [int]$w, [int]$h) {
  $script:id++
  $sid = $script:id
  $payload = @{ id = $sid; method = 'Page.captureScreenshot'; params = @{ format = 'png'; clip = @{ x = $x; y = $y; width = $w; height = $h; scale = 1 } } } | ConvertTo-Json -Depth 5 -Compress
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
        $out = "F:\zcode\projects\tokenstep\TokenStep\pictures\$name.png"
        [IO.File]::WriteAllBytes($out, $img)
        Write-Output "CLIP $name.png ($($img.Length) B)"
      }
      return
    }
  }
}

# Wait for sync.
$deadline0 = [DateTime]::UtcNow.AddSeconds(300)
while ([DateTime]::UtcNow -lt $deadline0) {
  $txt = Invoke-Eval 'JSON.stringify({ grid: !!document.querySelector(".today-design-bottom"), sync: (document.getElementById("syncText")||{textContent:""}).textContent.trim() })'
  if ($txt -and $txt.Contains('"grid":true')) { Write-Output ("READY=" + $txt); break }
  Start-Sleep -Seconds 15
}

# Scroll the bottom row into view and screenshot the full row.
$r = Invoke-Eval @'
(async () => {
  const grid = document.querySelector(".today-design-bottom");
  if (!grid) return JSON.stringify({ error: "no grid" });
  grid.scrollIntoView({ block: "center" });
  await new Promise((r) => setTimeout(r, 600));
  const r0 = grid.getBoundingClientRect();
  return JSON.stringify({ x: Math.round(r0.x), y: Math.round(r0.y), w: Math.round(r0.width), h: Math.round(r0.height) });
})()
'@
Write-Output ("ROW: " + $r)
$info = $r | ConvertFrom-Json
if ($info.h -gt 60) { ClipShot 'fix-bottom-row' $info.x $info.y $info.w ([Math]::Max(150, $info.h)) }
$ws.Dispose()
cmd /c "taskkill /F /T /IM tokenstep.exe" 2>&1 | Out-Null
Write-Output 'BOTTOM-SHOT DONE'
