# Fetch the pinned np into apps\shell\np\ so the bundler can ship it inside the installer.
#
# The hash in packaging\np-pin.json is not decorative: an installer carrying a half-downloaded or
# swapped-out backend would index somebody's notes with it, and nothing downstream could tell. So the
# download is verified before it is unpacked, and a marker file records which archive this folder holds.
#
#   .\scripts\fetch-np.ps1            # fetch, or say why nothing changed
#   .\scripts\fetch-np.ps1 -Force     # re-download even if the folder already has this build

[CmdletBinding()]
param(
    [switch]$Force
)

$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$pin = Get-Content -Raw (Join-Path $root 'packaging\np-pin.json') | ConvertFrom-Json
$triple = 'x86_64-pc-windows-msvc'

if (-not $pin.assets.PSObject.Properties.Name -contains $triple) {
    Write-Error "packaging\np-pin.json has no asset for $triple"
}
$asset = $pin.assets.$triple
$dest = Join-Path $root 'apps\shell\np'
$marker = Join-Path $dest '.fetched-from'
$target = Join-Path $dest $asset.binary

if ((-not $Force) -and (Test-Path $marker) -and (Test-Path $target) -and
    ((Get-Content -Raw $marker).Trim() -eq $asset.sha256)) {
    Write-Host "np already fetched from $($asset.archive)"
    return
}

New-Item -ItemType Directory -Force -Path $dest | Out-Null
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("jott-np-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null

try {
    $archivePath = Join-Path $tmp $asset.archive
    $url = "$($pin.release)/$($asset.archive)"
    Write-Host "downloading $url"
    Invoke-WebRequest -Uri $url -OutFile $archivePath -UseBasicParsing

    $actual = (Get-FileHash -Algorithm SHA256 $archivePath).Hash.ToLower()
    if ($actual -ne $asset.sha256) {
        Write-Error "$($asset.archive) hashed $actual, expected $($asset.sha256) — refusing to bundle it"
    }

    Write-Host "hash matches; unpacking $($asset.binary)"
    Expand-Archive -Path $archivePath -DestinationPath $tmp -Force
    Copy-Item -Force (Join-Path $tmp $asset.binary) $target
    Set-Content -NoNewline -Path $marker -Value $asset.sha256
    Write-Host "np is at $target, from $($asset.archive)"
}
finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
