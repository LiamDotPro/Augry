<#
.SYNOPSIS
  Package the release build as an MSIX for the Microsoft Store.

.DESCRIPTION
  Stages augry.exe, the shell integration resources and the Store
  icons into src-tauri/target/msix/layout, fills src-tauri/msix/AppxManifest.xml
  from src-tauri/msix/identity.json and tauri.conf.json, and runs makeappx from
  the Windows SDK. The unsigned .msix is what gets uploaded to Partner Center;
  the Store signs it.

  -SelfSign signs the package with a self-signed certificate so it can be
  installed locally for testing. Trusting that certificate needs an elevated
  shell (the script prints the command).

.EXAMPLE
  .\scripts\pack-msix.ps1 -Build
  .\scripts\pack-msix.ps1 -SelfSign
  .\scripts\pack-msix.ps1 -Version 1.0.0.0 -IdentityName 12345LiamRead.Augry -Publisher "CN=0123ABCD-..."
#>
[CmdletBinding()]
param(
  # Run `npm run tauri build -- --no-bundle` first.
  [switch]$Build,
  # Sign with a self-signed dev certificate for local install testing.
  [switch]$SelfSign,
  # Four-part package version. Defaults to tauri.conf.json version + ".0".
  [string]$Version,
  # Overrides for src-tauri/msix/identity.json.
  [string]$IdentityName,
  [string]$Publisher,
  [string]$PublisherDisplayName,
  # Where the .msix lands. Defaults to src-tauri/target/msix.
  [string]$OutDir
)

$ErrorActionPreference = 'Stop'

$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$tauriDir = Join-Path $root 'src-tauri'
$conf = Get-Content (Join-Path $tauriDir 'tauri.conf.json') -Raw | ConvertFrom-Json
$identity = Get-Content (Join-Path $tauriDir 'msix\identity.json') -Raw | ConvertFrom-Json

if (-not $IdentityName) { $IdentityName = $identity.identityName }
if (-not $Publisher) { $Publisher = $identity.publisher }
if (-not $PublisherDisplayName) { $PublisherDisplayName = $identity.publisherDisplayName }
if (-not $Version) { $Version = "$($conf.version).0" }
if (-not $OutDir) { $OutDir = Join-Path $tauriDir 'target\msix' }

if ($Version -notmatch '^\d+\.\d+\.\d+\.\d+$') {
  throw "Version must have four parts (got '$Version')."
}
if ($Version.Split('.')[0] -eq '0') {
  Write-Warning "The Store rejects packages whose major version is 0. '$Version' is fine for local testing only."
}
if ($Version.Split('.')[3] -ne '0') {
  Write-Warning "The Store reserves the fourth version part and expects it to be 0."
}

function Find-SdkTool([string]$Name) {
  $kits = 'C:\Program Files (x86)\Windows Kits\10\bin'
  $candidate = Get-ChildItem $kits -Directory -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -match '^10\.' } |
    Sort-Object { [version]$_.Name } -Descending |
    ForEach-Object { Join-Path $_.FullName "x64\$Name" } |
    Where-Object { Test-Path $_ } |
    Select-Object -First 1
  if (-not $candidate) {
    throw "$Name not found under $kits. Install the Windows 10/11 SDK (Visual Studio Installer > Individual components)."
  }
  $candidate
}

$makeappx = Find-SdkTool 'makeappx.exe'

if ($Build) {
  Push-Location $root
  try {
    npm run tauri build -- --no-bundle
    if ($LASTEXITCODE -ne 0) { throw "tauri build failed ($LASTEXITCODE)" }
  } finally { Pop-Location }
}

$exe = Join-Path $tauriDir 'target\release\augry.exe'
if (-not (Test-Path $exe)) {
  throw "Release exe not found at $exe. Run with -Build or run 'npm run tauri build' first."
}

# Stage the same layout the NSIS/MSI installers produce: exe at the root and
# the scripts under resources\, which is where resource_dir() resolves to.
$layout = Join-Path $tauriDir 'target\msix\layout'
if (Test-Path $layout) { Remove-Item $layout -Recurse -Force }
New-Item -ItemType Directory -Path (Join-Path $layout 'resources') | Out-Null
New-Item -ItemType Directory -Path (Join-Path $layout 'Assets') | Out-Null
Copy-Item $exe $layout
Copy-Item (Join-Path $tauriDir 'resources\*') (Join-Path $layout 'resources')
foreach ($asset in 'Square44x44Logo', 'Square71x71Logo', 'Square150x150Logo', 'StoreLogo') {
  Copy-Item (Join-Path $tauriDir "icons\$asset.png") (Join-Path $layout "Assets\$asset.png")
}

$manifest = Get-Content (Join-Path $tauriDir 'msix\AppxManifest.xml') -Raw
$manifest = $manifest.Replace('__IDENTITY_NAME__', $IdentityName)
$manifest = $manifest.Replace('__PUBLISHER__', $Publisher)
$manifest = $manifest.Replace('__PUBLISHER_DISPLAY_NAME__', $PublisherDisplayName)
$manifest = $manifest.Replace('__VERSION__', $Version)
$manifest = $manifest.Replace('__DISPLAY_NAME__', $conf.productName)
$manifest = $manifest.Replace('__DESCRIPTION__', $identity.description)
$utf8NoBom = New-Object System.Text.UTF8Encoding $false
[System.IO.File]::WriteAllText((Join-Path $layout 'AppxManifest.xml'), $manifest, $utf8NoBom)

New-Item -ItemType Directory -Path $OutDir -Force | Out-Null
$package = Join-Path $OutDir "$($conf.productName)_${Version}_x64.msix"
& $makeappx pack /d $layout /p $package /o
if ($LASTEXITCODE -ne 0) { throw "makeappx failed ($LASTEXITCODE)" }

if ($SelfSign) {
  $signtool = Find-SdkTool 'signtool.exe'
  $friendly = 'Augry MSIX dev'
  $cert = Get-ChildItem Cert:\CurrentUser\My |
    Where-Object { $_.Subject -eq $Publisher -and $_.FriendlyName -eq $friendly } |
    Select-Object -First 1
  if (-not $cert) {
    $cert = New-SelfSignedCertificate -Type Custom -Subject $Publisher -KeyUsage DigitalSignature `
      -FriendlyName $friendly -CertStoreLocation Cert:\CurrentUser\My `
      -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
  }
  & $signtool sign /fd SHA256 /sha1 $cert.Thumbprint $package
  if ($LASTEXITCODE -ne 0) { throw "signtool failed ($LASTEXITCODE)" }
  $cer = Join-Path $OutDir 'augry-dev.cer'
  Export-Certificate -Cert $cert -FilePath $cer | Out-Null
  Write-Host ''
  Write-Host 'Signed with a self-signed dev certificate. To install locally, trust it once from an elevated shell:'
  Write-Host "  Import-Certificate -FilePath '$cer' -CertStoreLocation Cert:\LocalMachine\TrustedPeople"
  Write-Host 'then:'
  Write-Host "  Add-AppxPackage '$package'"
  Write-Host 'Do not upload the signed package to the Store; build again without -SelfSign.'
}

Write-Host ''
Write-Host "MSIX: $package"
