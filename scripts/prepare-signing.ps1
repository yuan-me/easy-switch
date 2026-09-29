$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot -Parent
$secureRoot=Join-Path $root '.local'
New-Item -ItemType Directory -Path $secureRoot -Force | Out-Null
$keyPath=Join-Path $secureRoot 'updater.key'
if (!(Test-Path -LiteralPath $keyPath)) {
    Add-Type -AssemblyName System.Security
    $bytes=New-Object byte[] 48
    [Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($bytes)
    $password=[Convert]::ToBase64String($bytes)
    $protected=[Security.Cryptography.ProtectedData]::Protect([Text.Encoding]::UTF8.GetBytes($password),$null,[Security.Cryptography.DataProtectionScope]::CurrentUser)
    [IO.File]::WriteAllBytes((Join-Path $secureRoot 'updater-password.dpapi'),$protected)
    & (Join-Path $root 'node_modules/.bin/tauri.cmd') signer generate --ci --password $password --write-keys $keyPath *> (Join-Path $secureRoot 'key-generation.log')
    $password=$null; $bytes=$null
    if ($LASTEXITCODE -ne 0) { throw 'Signing key generation failed; inspect local result without publishing its contents.' }
}
$publicKey=[IO.File]::ReadAllText($keyPath+'.pub').Trim()
$configPath=Join-Path $root 'src-tauri/tauri.conf.json'
$config=Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
$config.plugins.updater.pubkey=$publicKey
[IO.File]::WriteAllText($configPath,($config | ConvertTo-Json -Depth 20),[Text.UTF8Encoding]::new($false))
Write-Output 'Signing key prepared; only public key embedded in application config.'
