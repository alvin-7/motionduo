param(
    [Parameter(Mandatory=$true)][string]$InnoCompiler,
    [Parameter(Mandatory=$true)][string]$VCRuntimeRoot,
    [Parameter(Mandatory=$true)][string]$RuntimeLicenseDir,
    [string]$Go = 'go'
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
if ((Get-Location).Path -ne $repo) { throw 'Run from the repository root' }
function Invoke-PackagingTool([string]$Command, [string[]]$Arguments, [string]$LogPath) {
    $ErrorActionPreference = 'Continue'
    & $Command @Arguments *> $LogPath
    $code = $LASTEXITCODE
    if ($code -ne 0) { throw "Tool failed ($code): $Command; see $LogPath" }
}
foreach ($item in @($InnoCompiler, $VCRuntimeRoot, $RuntimeLicenseDir)) {
    if (!(Test-Path -LiteralPath $item)) { throw "Missing packaging prerequisite: $item" }
}
$metadata = & cargo metadata --no-deps --format-version 1 --locked | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Cannot read Cargo package metadata' }
$version = ($metadata.packages | Where-Object name -eq 'mousehop').version
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw "Unsupported installer version: $version" }
$output = Join-Path $repo "dist/windows-$version-$(Get-Date -Format yyyyMMdd-HHmmss)"
if (Test-Path -LiteralPath $output) { throw "Refusing to overwrite $output" }
$stage = Join-Path $output 'stage'
New-Item -ItemType Directory -Path "$stage/bin" | Out-Null
& "$PSScriptRoot/build-mygo.ps1" -Release -Go $Go *> "$output/build.log"
if ($LASTEXITCODE -ne 0) { throw "Native build failed: $output/build.log" }
Copy-Item -LiteralPath "$($metadata.target_directory)/release/motionduo.exe", "$repo/target/release/motionduo-ui.exe" -Destination "$stage/bin"
Get-ChildItem "$VCRuntimeRoot/*140*.dll" -File | Copy-Item -Destination "$stage/bin"
Copy-Item -LiteralPath $RuntimeLicenseDir -Destination "$stage/runtime-licenses" -Recurse
Copy-Item -LiteralPath "$repo/LICENSE", "$repo/NOTICE" -Destination $stage
Copy-Item -LiteralPath "$repo/mousehop-mygo/THIRD_PARTY_NOTICES.md" -Destination $stage
Invoke-PackagingTool "$stage/bin/motionduo.exe" @('--version') "$output/launch-version.log"
Invoke-PackagingTool "$stage/bin/motionduo-ui.exe" @('--version') "$output/frontend-version.log"
Invoke-PackagingTool $InnoCompiler @("/DStageDir=$stage", "/DOutputDir=$output", "/DAppVersion=$version", "$repo/packaging/windows.iss") "$output/inno.log"
$installer = Join-Path $output "MotionDuo-$version-x64-Setup.exe"
$hash = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
"$hash  $(Split-Path $installer -Leaf)" | Set-Content "$output/SHA256.txt" -Encoding UTF8
Write-Output "PACKAGE=$installer"
