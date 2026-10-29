param(
    [switch]$Release,
    [string]$Go = 'go'
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
if ((Get-Location).Path -ne $repo) { throw 'Run from the repository root' }
$goVersion = & $Go version
if ($LASTEXITCODE -ne 0) { throw 'Go is not available' }
if ($goVersion -notmatch 'go1\.(\d+)' -or [int]$Matches[1] -lt 27) {
    $cached = Get-ChildItem -Path "$HOME/go/pkg/mod/golang.org/toolchain@v0.0.1-go1.27.*.windows-amd64/bin/go.exe" -ErrorAction SilentlyContinue | Sort-Object FullName -Descending | Select-Object -First 1
    if (!$cached) { throw 'Go 1.27.3+ is required. Use -Go with a compatible go.exe path.' }
    $Go = $cached.FullName
}
$profile = if ($Release) { 'release' } else { 'debug' }
$cargoArgs = @('build', '--workspace', '--locked')
if ($Release) { $cargoArgs += '--release' }
& cargo @cargoArgs
if ($LASTEXITCODE -ne 0) { throw 'Rust build failed' }
$output = Join-Path $repo "target/$profile/motionduo-ui.exe"
$metadata = & cargo metadata --no-deps --format-version 1 --locked | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Cannot read Cargo package metadata' }
$version = ($metadata.packages | Where-Object name -eq 'mousehop').version
$arch = & $Go -C mousehop-mygo env GOARCH
if ($LASTEXITCODE -ne 0 -or $arch -notin @('amd64', 'arm64', '386')) { throw "Unsupported Windows Go architecture: $arch" }
$resource = Join-Path $repo "mousehop-mygo/mousehop_windows_$arch.syso"
if (Test-Path -LiteralPath $resource) { throw "Refusing to overwrite existing resources: $resource" }
try {
    & $Go -C mousehop-mygo tool rsrc -arch $arch -manifest windows.manifest -ico ../assets/brand/motionduo.ico -o $resource
    if ($LASTEXITCODE -ne 0) { throw 'Windows icon resource generation failed' }
    & $Go -C mousehop-mygo build -mod=readonly -trimpath -ldflags "-H=windowsgui -X main.version=$version" -o $output .
    if ($LASTEXITCODE -ne 0) { throw 'MyGo build failed' }
} finally {
    if (Test-Path -LiteralPath $resource) { Remove-Item -LiteralPath $resource }
}
Write-Output "Built target/$profile/motionduo.exe and target/$profile/motionduo-ui.exe"
