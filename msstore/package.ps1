<#
Builds the app without Steam and packs it as an MSIX for the Microsoft Store.
Run on Windows, with Rust, the Tauri CLI and the Windows SDK (for makepri,
makeappx and signtool):

  $env:MSSTORE_IDENTITY_NAME = '...'           # Package/Identity/Name
  $env:MSSTORE_PUBLISHER = 'CN=...'            # Package/Identity/Publisher
  $env:MSSTORE_PUBLISHER_DISPLAY_NAME = '...'  # Package/Properties/PublisherDisplayName
  .\msstore\package.ps1

The three values are in Partner Center (Product management > Product
identity). The package goes to target\msstore\ unsigned, as the Store takes
it: the Store signs it. To install it on this machine first, sign it with a
certificate whose subject is MSSTORE_PUBLISHER and that the machine trusts:

  .\msstore\package.ps1 -Pfx dev.pfx           # password in $env:MSSTORE_PFX_PASSWORD
#>
param(
  [string]$Pfx
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = Split-Path -Parent $PSScriptRoot
$out = Join-Path $root 'target\msstore'
$layout = Join-Path $out 'layout'

function Need([string]$name) {
  $value = [Environment]::GetEnvironmentVariable($name)
  if (-not $value) { throw "set $name (Partner Center > Product management > Product identity)" }
  $value
}

# The newest copy of a tool in the Windows SDK.
function SdkTool([string]$exe) {
  $found = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\10.*\x64\$exe" -ErrorAction SilentlyContinue |
    Sort-Object { [version]$_.Directory.Parent.Name } |
    Select-Object -Last 1
  if (-not $found) { throw "no ${exe}: install the Windows SDK" }
  $found.FullName
}

function Run([string]$exe) {
  & $exe @args
  if ($LASTEXITCODE -ne 0) { throw "$(Split-Path -Leaf $exe) failed ($LASTEXITCODE)" }
}

$identity = Need 'MSSTORE_IDENTITY_NAME'
$publisher = Need 'MSSTORE_PUBLISHER'
$publisherDisplayName = Need 'MSSTORE_PUBLISHER_DISPLAY_NAME'

# The Store keeps a package version's fourth part for itself, and it must
# be 0.
$version = (Get-Content (Join-Path $root 'tauri.conf.json') -Raw | ConvertFrom-Json).version
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw "tauri.conf.json's version $version is not major.minor.patch" }
$packageVersion = "$version.0"

$makepri = SdkTool 'makepri.exe'
$makeappx = SdkTool 'makeappx.exe'

# Without Steam: its library is not in the package, and the app never asks
# for it (README.md).
Push-Location $root
try {
  Run 'cargo' 'tauri' 'build' '--no-bundle' '--' '--no-default-features'
} finally {
  Pop-Location
}

# What goes in the package: the program, its images and its manifest.
if (Test-Path $layout) { Remove-Item $layout -Recurse -Force }
New-Item $layout -ItemType Directory | Out-Null
Copy-Item (Join-Path $root 'target\release\oeee-cafe-desktop.exe') $layout
Copy-Item (Join-Path $PSScriptRoot 'Assets') $layout -Recurse

$manifest = Get-Content (Join-Path $PSScriptRoot 'AppxManifest.xml') -Raw
$manifest = $manifest.Replace('$IDENTITY_NAME$', [Security.SecurityElement]::Escape($identity))
$manifest = $manifest.Replace('$PUBLISHER$', [Security.SecurityElement]::Escape($publisher))
$manifest = $manifest.Replace('$PUBLISHER_DISPLAY_NAME$', [Security.SecurityElement]::Escape($publisherDisplayName))
$manifest = $manifest.Replace('$VERSION$', $packageVersion)
[IO.File]::WriteAllText((Join-Path $layout 'AppxManifest.xml'), $manifest, [Text.UTF8Encoding]::new($false))

# The index Windows picks each image's variant from (assets.sh names them).
$priconfig = Join-Path $out 'priconfig.xml'
Run $makepri 'createconfig' '/cf' $priconfig '/dq' 'en-US' '/pv' '10.0.0' '/o'
Run $makepri 'new' '/pr' $layout '/cf' $priconfig '/mn' (Join-Path $layout 'AppxManifest.xml') '/of' (Join-Path $layout 'resources.pri') '/o'

$msix = Join-Path $out "OeeeCafe_${packageVersion}_x64.msix"
Run $makeappx 'pack' '/d' $layout '/p' $msix '/o'

if ($Pfx) {
  $signtool = SdkTool 'signtool.exe'
  $sign = @('sign', '/fd', 'SHA256', '/f', (Resolve-Path $Pfx).Path)
  if ($env:MSSTORE_PFX_PASSWORD) { $sign += @('/p', $env:MSSTORE_PFX_PASSWORD) }
  Run $signtool @sign $msix
}

Write-Host "packed $msix"
