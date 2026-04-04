param(
    [string]$BackendUrl = "http://localhost:3000"
)

Write-Host "=== Login Test ==="

$uri = $BackendUrl + "/api/v1/auth/login"
$loginHash = @{ email = "admin@localhost"; password = "admin" }
$body = ($loginHash | ConvertTo-Json -Compress)
Write-Host ("POST " + $uri)
Write-Host ("Body: " + $body)

$req = [System.Net.HttpWebRequest]::Create($uri)
$req.Method = "POST"
$req.ContentType = "application/json"
$bytes = [System.Text.Encoding]::UTF8.GetBytes($body)
$req.ContentLength = $bytes.Length
$s = $req.GetRequestStream()
$s.Write($bytes, 0, $bytes.Length)
$s.Close()

$resp = $req.GetResponse()
$reader = New-Object System.IO.StreamReader($resp.GetResponseStream())
$raw = $reader.ReadToEnd()
$reader.Close()
$resp.Close()

Write-Host ("RAW (" + $raw.Length + " chars): " + $raw)

$parsed = ($raw | ConvertFrom-Json)
Write-Host ("Parsed token length: " + $parsed.token.Length)
Write-Host ("Token start: " + $parsed.token.Substring(0, 20))

Write-Host ""
Write-Host "=== GET /sites Test ==="

$uri2 = $BackendUrl + "/api/v1/sites"
$req2 = [System.Net.HttpWebRequest]::Create($uri2)
$req2.Method = "GET"
$req2.Headers.Add("Authorization", "Bearer " + $parsed.token)

try {
    $resp2 = $req2.GetResponse()
    $reader2 = New-Object System.IO.StreamReader($resp2.GetResponseStream())
    $raw2 = $reader2.ReadToEnd()
    $reader2.Close()
    $resp2.Close()
    Write-Host ("GET /sites OK: " + $raw2)
} catch {
    Write-Host ("GET /sites FAILED: " + $_.Exception.Message) -ForegroundColor Red
}
