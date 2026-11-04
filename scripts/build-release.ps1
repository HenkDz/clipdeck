param([Parameter(Mandatory=$true)][string]$Notes, [int]$Schema = 5)
$ErrorActionPreference = 'Stop'
Push-Location (Split-Path $PSScriptRoot)
try {
    foreach ($arguments in @(@('fmt','--check'),@('clippy','--all-targets','--','-D','warnings'),@('test'),@('build','--release'))) {
        & cargo @arguments
        if ($LASTEXITCODE -ne 0) { throw "cargo $arguments failed" }
    }
    $versionMatch = Select-String -Path Cargo.toml -Pattern '^version = "([^"]+)"$' | Select-Object -First 1
    $version = $versionMatch.Matches[0].Groups[1].Value
    if ((Get-Item target/release/clipdeck.exe).VersionInfo.ProductVersion -ne $version) {
        throw 'The compiled executable version differs from Cargo.toml.'
    }
    & python scripts/package-release.py --notes $Notes --schema $Schema
    if ($LASTEXITCODE -ne 0) { throw 'Release packaging or signing failed' }
} finally { Pop-Location }
