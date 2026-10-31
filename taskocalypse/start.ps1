$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot
if (-not (Test-Path -LiteralPath '.env')) {
    Copy-Item -LiteralPath '.env.example' -Destination '.env'
    node -e "const fs=require('fs');const p='.env';fs.writeFileSync(p,fs.readFileSync(p,'utf8').replace('TOKEN_ENCRYPTION_KEY=','TOKEN_ENCRYPTION_KEY='+require('crypto').randomBytes(32).toString('base64')));"
    Write-Host 'Created a private local .env. Add your Google credentials there when ready.'
}
if (-not (Test-Path -LiteralPath 'dist/index.html')) {
    npm.cmd ci
    if ($LASTEXITCODE -ne 0) { throw 'Dependency installation failed.' }
    npm.cmd run build
    if ($LASTEXITCODE -ne 0) { throw 'Frontend build failed.' }
}
if (Test-Path -LiteralPath '.local/cargo/bin/cargo.exe') {
    $env:CARGO_HOME = Join-Path $PSScriptRoot '.local/cargo'
    $env:RUSTUP_HOME = Join-Path $PSScriptRoot '.local/rustup'
    $env:Path = (Join-Path $PSScriptRoot '.local/cargo/bin') + ';' + $env:Path
}
cargo run --manifest-path server/Cargo.toml
