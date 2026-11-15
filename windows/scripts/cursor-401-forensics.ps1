# Cursor quota 401 forensics: credential freshness + live API re-test (ASCII-only).
$ErrorActionPreference = 'Continue'
Write-Output '=== 1) state.vscdb freshness ==='
$vscdb = Join-Path $env:USERPROFILE '.cursor\User\globalStorage\state.vscdb'
if (Test-Path $vscdb) {
  $f = Get-Item $vscdb
  Write-Output ('path ok: ' + [math]::Round($f.Length/1MB,1) + ' MB, mtime ' + $f.LastWriteTime.ToString('yyyy-MM-dd HH:mm:ss'))
} else {
  Write-Output 'MISSING: default path'
  Get-ChildItem (Join-Path $env:USERPROFILE '.cursor') -Recurse -Filter 'state.vscdb' -ErrorAction SilentlyContinue | Select-Object -First 3 | ForEach-Object { Write-Output ('alt: ' + $_.FullName) }
}
Write-Output '=== 2) token fields in ItemTable ==='
python -I -c "
import sqlite3, os
p = os.path.expanduser('~/.cursor/User/globalStorage/state.vscdb')
db = sqlite3.connect('file:' + p.replace(os.sep, '/') + '?mode=ro', uri=True)
rows = db.execute(\"SELECT key, length(value) FROM ItemTable WHERE key LIKE '%cursorAuth%' OR key LIKE '%token%'\").fetchall()
for k, l in rows:
    print(k, 'len:', l)
" 2>&1 | head -8
Write-Output '=== 3) live API test with the stored cookie ==='
python -I -c "
import sqlite3, os, json, urllib.request
p = os.path.expanduser('~/.cursor/User/globalStorage/state.vscdb')
db = sqlite3.connect('file:' + p.replace(os.sep, '/') + '?mode=ro', uri=True)
tok = db.execute(\"SELECT value FROM ItemTable WHERE key='cursorAuth/accessToken' LIMIT 1\").fetchone()[0]
uid_row = db.execute(\"SELECT value FROM ItemTable WHERE key='cursorAuth/userId' LIMIT 1\").fetchone()
uid = uid_row[0] if uid_row else ''
print('token len:', len(tok), 'userId:', uid[:12] if uid else '(empty)')
req = urllib.request.Request('https://cursor.com/api/usage-summary', headers={'Accept': 'application/json', 'Cookie': 'WorkosCursorSessionToken=' + uid + '::' + tok, 'User-Agent': 'Mozilla/5.0'})
try:
    r = urllib.request.urlopen(req, timeout=20)
    body = r.read(400).decode('utf-8', 'replace')
    print('HTTP', r.status, body[:200])
except Exception as e:
    print('ERR:', type(e).__name__, str(e)[:200])
" 2>&1 | head -12
