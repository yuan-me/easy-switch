$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot -Parent
$fixtures=Join-Path $root 'tests/fixtures'
New-Item -ItemType Directory -Path $fixtures -Force | Out-Null
$key=Join-Path $root '.local/fixture-signing.key'
$cli=Join-Path $root 'node_modules/.bin/tauri.cmd'
if(!(Test-Path -LiteralPath $key)){ & $cli signer generate --ci -w $key *> (Join-Path $root '.local/fixture-signing.log'); if($LASTEXITCODE -ne 0){throw 'Fixture signing setup failed'} }
[IO.File]::WriteAllText((Join-Path $fixtures 'update.payload'),'Easy Switch synthetic update fixture',[Text.UTF8Encoding]::new($false))
Copy-Item -LiteralPath ($key+'.pub') -Destination (Join-Path $fixtures 'update.pub')
& $cli signer sign -f $key --app-version 2.0.0 (Join-Path $fixtures 'update.payload') *> (Join-Path $root '.local/fixture-signing.log')
if($LASTEXITCODE -ne 0){throw 'Fixture signing failed'}
Write-Output 'Synthetic signature fixtures ready; private fixture key excluded from Git.'
