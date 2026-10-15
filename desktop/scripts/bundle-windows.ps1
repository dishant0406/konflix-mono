# Builds the Windows app and packs it, with libmpv beside it, into target\release\bundle\
# Akidev-<version>-x86_64.zip, and into Akidev-<version>-x86_64-setup.exe when Inno Setup is there. Run from a Visual Studio developer prompt (the CI job does).
#
# libmpv is not on the machine: the newest mpv-dev build of shinchiro/mpv-winbuild-cmake is
# downloaded into target\mpv (kept between runs). Its import library is made from the DLL's
# exports, so the MSVC linker does not depend on how the build's own library was written.
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')

$mpv = Join-Path (Get-Location) 'target\mpv'
if (-not (Test-Path "$mpv\libmpv-2.dll")) {
    New-Item -ItemType Directory -Force $mpv | Out-Null
    # CI machines share addresses, and GitHub limits anonymous requests per address: a token
    # (GITHUB_TOKEN in the CI job) lifts that, and a few retries cover the rest.
    $headers = @{ 'User-Agent' = 'akidev-build' }
    if ($env:GITHUB_TOKEN) { $headers['Authorization'] = "Bearer $env:GITHUB_TOKEN" }
    $release = $null
    foreach ($attempt in 1..5) {
        try {
            $release = Invoke-RestMethod 'https://api.github.com/repos/shinchiro/mpv-winbuild-cmake/releases/latest' -Headers $headers
            break
        } catch {
            if ($attempt -eq 5) { throw }
            Start-Sleep -Seconds (10 * $attempt)
        }
    }
    $asset = $release.assets | Where-Object { $_.name -match '^mpv-dev-x86_64-\d' } | Select-Object -First 1
    Write-Host "libmpv: $($asset.name)"
    $archive = Join-Path $mpv $asset.name
    Invoke-WebRequest $asset.browser_download_url -OutFile $archive
    & 7z x $archive "-o$mpv" -y | Out-Null
    Remove-Item $archive
}
if (-not (Test-Path "$mpv\mpv.lib")) {
    $names = & dumpbin /exports "$mpv\libmpv-2.dll" |
        ForEach-Object { if ($_ -match '^\s+\d+\s+[0-9A-F]+\s+[0-9A-F]{8}\s+(\S+)') { $Matches[1] } }
    if (-not $names) { throw 'libmpv-2.dll exports nothing' }
    "LIBRARY libmpv-2.dll`r`nEXPORTS`r`n" + ($names -join "`r`n") | Set-Content "$mpv\mpv.def" -Encoding ascii
    & lib /nologo "/def:$mpv\mpv.def" "/out:$mpv\mpv.lib" /machine:x64
    if ($LASTEXITCODE -ne 0) { throw 'lib.exe failed' }
}

$env:LIB = "$mpv;$env:LIB"
cargo build --release -p akidev
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

$version = (cargo pkgid -p akidev) -replace '.*[#@]', ''
$stage = Join-Path (Get-Location) 'target\release\bundle\Akidev'
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item target\release\akidev.exe "$stage\Akidev.exe"
Copy-Item "$mpv\libmpv-2.dll" $stage
@"
Akidev for Windows

Unzip this folder anywhere and run Akidev.exe. Nothing is installed.

Windows SmartScreen may say the app is unrecognised, because it is not signed with a
code-signing certificate yet: press "More info", then "Run anyway".
"@ | Set-Content "$stage\Read me first.txt" -Encoding utf8

$zip = Join-Path (Get-Location) "target\release\bundle\Akidev-$version-x86_64.zip"
Remove-Item $zip -ErrorAction SilentlyContinue
Compress-Archive -Path "$stage\*" -DestinationPath $zip
Write-Host "Built $zip"

# The installer, when Inno Setup is installed (the CI job installs it).
$iscc = @("${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe") |
    Where-Object { Test-Path $_ } | Select-Object -First 1
if ($iscc) {
    & $iscc "/DAppVersion=$version" scripts\akidev.iss
    if ($LASTEXITCODE -ne 0) { throw 'Inno Setup failed' }
} else {
    Write-Host 'Inno Setup is not installed: no installer was made.'
}
