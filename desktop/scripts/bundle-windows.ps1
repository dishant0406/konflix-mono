# Builds the Windows app and packs it, with libmpv beside it, into target\release\bundle\
# Akidev-<version>-x86_64.zip. Run from a Visual Studio developer prompt (the CI job does).
#
# libmpv is not on the machine: the newest mpv-dev build of shinchiro/mpv-winbuild-cmake is
# downloaded into target\mpv (kept between runs). Its import library is made from the DLL's
# exports, so the MSVC linker does not depend on how the build's own library was written.
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')

$mpv = Join-Path (Get-Location) 'target\mpv'
if (-not (Test-Path "$mpv\libmpv-2.dll")) {
    New-Item -ItemType Directory -Force $mpv | Out-Null
    $release = Invoke-RestMethod 'https://api.github.com/repos/shinchiro/mpv-winbuild-cmake/releases/latest'
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
