$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot -Parent
Set-Location -LiteralPath $root
$localTools=Join-Path (Split-Path $root -Parent) '.tools'
if(Test-Path -LiteralPath (Join-Path $localTools 'cargo/bin/cargo.exe')) {
    $env:CARGO_HOME=Join-Path $localTools 'cargo';$env:RUSTUP_HOME=Join-Path $localTools 'rustup';$env:PATH=(Join-Path $env:CARGO_HOME 'bin')+';'+$env:PATH
}
Add-Type -AssemblyName System.Security
$key=Join-Path $root '.local/updater.key'
$passwordFile=Join-Path $root '.local/updater-password.dpapi'
if(!(Test-Path -LiteralPath $key)){throw 'Run prepare-signing.ps1 first.'}
$env:TAURI_SIGNING_PRIVATE_KEY=[IO.File]::ReadAllText($key).Trim()
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD=[Text.Encoding]::UTF8.GetString([Security.Cryptography.ProtectedData]::Unprotect([IO.File]::ReadAllBytes($passwordFile),$null,[Security.Cryptography.DataProtectionScope]::CurrentUser))
try {
    npm.cmd run tauri -- build --bundles nsis
    if($LASTEXITCODE -ne 0){throw 'Signed installer build failed'}
}finally{$env:TAURI_SIGNING_PRIVATE_KEY=$null;$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD=$null}
