param([switch]$Installer)
$ErrorActionPreference='Stop'
Set-Location -LiteralPath (Split-Path $PSScriptRoot -Parent)
$localTools=Join-Path (Split-Path $PWD -Parent) '.tools'
if(Test-Path -LiteralPath (Join-Path $localTools 'cargo/bin/cargo.exe')) {
  $env:CARGO_HOME=Join-Path $localTools 'cargo'
  $env:RUSTUP_HOME=Join-Path $localTools 'rustup'
  $env:PATH=(Join-Path $env:CARGO_HOME 'bin')+';'+$env:PATH
}
npm.cmd ci --no-audit --no-fund
if($LASTEXITCODE -ne 0){throw 'npm ci failed'}
npm.cmd run build
if($LASTEXITCODE -ne 0){throw 'Frontend build failed'}
cargo test -p easy-switch-core --locked
if($LASTEXITCODE -ne 0){throw 'Core tests failed'}
if($Installer){npm.cmd run tauri -- build --bundles nsis}else{npm.cmd run tauri -- build --no-bundle}
if($LASTEXITCODE -ne 0){throw 'Desktop build failed'}
