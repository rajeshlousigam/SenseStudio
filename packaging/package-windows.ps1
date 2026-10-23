# Builds a portable SenseStudio .zip for Windows x64.
#   pwsh packaging/package-windows.ps1 -QtDir <qt-prefix>
# Run from an MSVC developer shell (cl.exe on PATH). <qt-prefix> contains bin\windeployqt.exe.
# Bundles static FFmpeg/ffprobe (BtbN GPL build) and standalone yt-dlp next to the exe.
param([Parameter(Mandatory = $true)][string]$QtDir)

$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path "$PSScriptRoot\..").Path
$Version = (Select-String -Path "$Root\app\CMakeLists.txt" -Pattern 'SENSE_VERSION_LABEL "([^"]+)"').Matches[0].Groups[1].Value
$Build = "$Root\app\build-windows"
$Tools = "$Root\.cache\tools\windows-x64"
$Dist = "$Root\dist"
$Name = "SenseStudio-$Version-windows-x64"
$Stage = "$Dist\$Name"

# BtbN publishes the 9.0 release branch under a rolling "latest" tag.
$FfmpegUrl = 'https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-n9.0-latest-win64-gpl-9.0.zip'
$YtdlpUrl = 'https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe'

New-Item -ItemType Directory -Force -Path $Tools, $Dist | Out-Null

Write-Host '==> Tools'
if (-not (Test-Path "$Tools\ffmpeg.exe")) {
    $zip = "$Tools\ffmpeg.zip"
    Invoke-WebRequest -Uri $FfmpegUrl -OutFile $zip
    Expand-Archive -Path $zip -DestinationPath "$Tools\ffmpeg-x" -Force
    Get-ChildItem "$Tools\ffmpeg-x" -Recurse -Include ffmpeg.exe, ffprobe.exe | Copy-Item -Destination $Tools
    Remove-Item $zip, "$Tools\ffmpeg-x" -Recurse -Force
}
if (-not (Test-Path "$Tools\yt-dlp.exe")) {
    Invoke-WebRequest -Uri $YtdlpUrl -OutFile "$Tools\yt-dlp.exe"
}

Write-Host "==> Build $Version"
cmake -S "$Root\app" -B $Build -G Ninja -DCMAKE_BUILD_TYPE=Release "-DCMAKE_PREFIX_PATH=$QtDir"
if ($LASTEXITCODE -ne 0) { throw 'cmake configure failed' }
cmake --build $Build
if ($LASTEXITCODE -ne 0) { throw 'cmake build failed' }

Write-Host '==> Stage'
if (Test-Path $Stage) { Remove-Item $Stage -Recurse -Force }
New-Item -ItemType Directory -Path $Stage | Out-Null
Copy-Item "$Build\SenseStudio.exe", "$Build\sense-cli.exe" -Destination $Stage
Copy-Item "$Tools\ffmpeg.exe", "$Tools\ffprobe.exe", "$Tools\yt-dlp.exe" -Destination $Stage

Write-Host '==> Deploy Qt'
& "$QtDir\bin\windeployqt.exe" --release --qmldir "$Root\app\qml" --no-translations --compiler-runtime "$Stage\SenseStudio.exe"
if ($LASTEXITCODE -ne 0) { throw 'windeployqt failed' }

foreach ($exe in 'SenseStudio.exe', 'sense-cli.exe', 'ffmpeg.exe', 'ffprobe.exe', 'yt-dlp.exe') {
    if (-not (Test-Path "$Stage\$exe")) { throw "missing $exe in package" }
}

Write-Host '==> Zip'
$ZipPath = "$Dist\$Name.zip"
if (Test-Path $ZipPath) { Remove-Item $ZipPath -Force }
Compress-Archive -Path $Stage -DestinationPath $ZipPath
Write-Host "Built $ZipPath"
