param([switch]$Initialize, [Parameter(Mandatory=$true)][string]$Manifest)
$ErrorActionPreference = 'Stop'
$keyName = 'Clipdeck.UpdateSigning.v1'
$provider = [System.Security.Cryptography.CngProvider]::MicrosoftSoftwareKeyStorageProvider
if (-not [System.Security.Cryptography.CngKey]::Exists($keyName, $provider)) {
    if (-not $Initialize) { throw 'Update signing key is missing. Initialize it on the release owner account first.' }
    $options = New-Object System.Security.Cryptography.CngKeyCreationParameters
    $options.Provider = $provider
    $options.KeyUsage = [System.Security.Cryptography.CngKeyUsages]::Signing
    $key = [System.Security.Cryptography.CngKey]::Create([System.Security.Cryptography.CngAlgorithm]::ECDsaP256, $keyName, $options)
} else {
    $key = [System.Security.Cryptography.CngKey]::Open($keyName, $provider)
}
try {
    $public = $key.Export([System.Security.Cryptography.CngKeyBlobFormat]::EccPublicBlob)
    $publicPath = Join-Path (Split-Path $PSScriptRoot) 'assets/update-public-key.bin'
    if (Test-Path -LiteralPath $publicPath) {
        if ([Convert]::ToBase64String([IO.File]::ReadAllBytes($publicPath)) -ne [Convert]::ToBase64String($public)) {
            throw 'This account has a different signing key. Refusing to change the trusted public key.'
        }
    } elseif ($Initialize) {
        [IO.File]::WriteAllBytes($publicPath, $public)
    } else { throw 'Trusted public key is missing.' }
    $path = (Resolve-Path -LiteralPath $Manifest).Path
    $hash = [System.Security.Cryptography.SHA256]::Create()
    try { $digest = $hash.ComputeHash([IO.File]::ReadAllBytes($path)) } finally { $hash.Dispose() }
    $signer = New-Object System.Security.Cryptography.ECDsaCng($key)
    try {
        $signature = $signer.SignHash($digest)
        if (-not $signer.VerifyHash($digest, $signature)) { throw 'Signature self-check failed.' }
    } finally { $signer.Dispose() }
    if ($signature.Length -ne 64) { throw 'Unexpected P-256 signature format.' }
    [IO.File]::WriteAllBytes("$path.sig", $signature)
    Write-Output "Signed $path (private key remains in the Windows user key store)"
} finally { $key.Dispose() }
