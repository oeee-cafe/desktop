<#
Builds the app without Steam, for x64 and ARM64, and packs both into an
MSIX bundle for the Microsoft Store. Run on Windows, with Rust and both its
Windows targets, the Tauri CLI, Visual Studio's C++ build tools for x64 and
ARM64, and the Windows SDK (for makepri, makeappx and signtool):

  rustup target add x86_64-pc-windows-msvc aarch64-pc-windows-msvc
  $env:MSSTORE_IDENTITY_NAME = '...'           # Package/Identity/Name
  $env:MSSTORE_PUBLISHER = 'CN=...'            # Package/Identity/Publisher
  $env:MSSTORE_PUBLISHER_DISPLAY_NAME = '...'  # Package/Properties/PublisherDisplayName
  .\msstore\package.ps1

The three values are in Partner Center (Product management > Product
identity). The bundle goes to target\msstore\ unsigned, as the Store takes
it: the Store signs it. To install it on this machine first, sign it with a
certificate whose subject is MSSTORE_PUBLISHER and that the machine trusts:

  .\msstore\package.ps1 -Pfx dev.pfx           # password in $env:MSSTORE_PFX_PASSWORD

-Architectures x64 (or arm64) builds one only, to try it quicker.
#>
param(
  [string]$Pfx,
  [ValidateSet('x64', 'arm64')]
  [string[]]$Architectures = @('x64', 'arm64')
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = Split-Path -Parent $PSScriptRoot
$out = Join-Path $root 'target\msstore'

# Each package's architecture, as the manifest names it, and Rust's.
$triples = @{
  'x64'   = 'x86_64-pc-windows-msvc'
  'arm64' = 'aarch64-pc-windows-msvc'
}

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
  # Cargo and the SDK tools write progress to stderr, which Windows
  # PowerShell turns into a terminating error under 'Stop' whenever the
  # output is captured (a CI log, a pipe); the exit code is what says.
  $ErrorActionPreference = 'Continue'
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

$template = Get-Content (Join-Path $PSScriptRoot 'AppxManifest.xml') -Raw
$template = $template.Replace('$IDENTITY_NAME$', [Security.SecurityElement]::Escape($identity))
$template = $template.Replace('$PUBLISHER$', [Security.SecurityElement]::Escape($publisher))
$template = $template.Replace('$PUBLISHER_DISPLAY_NAME$', [Security.SecurityElement]::Escape($publisherDisplayName))
$template = $template.Replace('$VERSION$', $packageVersion)

# The index Windows picks each image's variant from (assets.sh names them).
$priconfig = Join-Path $out 'priconfig.xml'
New-Item $out -ItemType Directory -Force | Out-Null
Run $makepri 'createconfig' '/cf' $priconfig '/dq' 'en-US' '/pv' '10.0.0' '/o'
# makepri's config splits each scale and language out into a resource
# package of its own, which this one package would not carry: the 200% and
# 400% images would be left out of its index, and a high-DPI display shown
# the 100% ones stretched. Everything stays in the one index instead.
$config = [xml](Get-Content $priconfig -Raw)
$packaging = $config.SelectSingleNode('//packaging')
if ($packaging) { [void]$packaging.ParentNode.RemoveChild($packaging) }
$config.Save($priconfig)

# One package per architecture, all in one folder for the bundle.
$packages = Join-Path $out 'packages'
if (Test-Path $packages) { Remove-Item $packages -Recurse -Force }
New-Item $packages -ItemType Directory | Out-Null

foreach ($arch in $Architectures) {
  $triple = $triples[$arch]

  # Without Steam: its library is not in the package, and the app never
  # asks for it (README.md).
  Push-Location $root
  try {
    Run 'cargo' 'tauri' 'build' '--no-bundle' '--target' $triple '--' '--no-default-features'
  } finally {
    Pop-Location
  }

  # What goes in the package: the program, its images and its manifest.
  $layout = Join-Path $out "layout-$arch"
  if (Test-Path $layout) { Remove-Item $layout -Recurse -Force }
  New-Item $layout -ItemType Directory | Out-Null
  Copy-Item (Join-Path $root "target\$triple\release\oeee-cafe-desktop.exe") $layout
  Copy-Item (Join-Path $PSScriptRoot 'Assets') $layout -Recurse

  $manifest = $template.Replace('$ARCHITECTURE$', $arch)
  [IO.File]::WriteAllText((Join-Path $layout 'AppxManifest.xml'), $manifest, [Text.UTF8Encoding]::new($false))
  Run $makepri 'new' '/pr' $layout '/cf' $priconfig '/mn' (Join-Path $layout 'AppxManifest.xml') '/of' (Join-Path $layout 'resources.pri') '/o'

  Run $makeappx 'pack' '/d' $layout '/p' (Join-Path $packages "OeeeCafe_${packageVersion}_$arch.msix") '/o'
}

# The Store takes one upload for every architecture, and Windows installs
# the package in it that suits the machine.
$bundle = Join-Path $out "OeeeCafe_${packageVersion}.msixbundle"
Run $makeappx 'bundle' '/d' $packages '/p' $bundle '/bv' $packageVersion '/o'

if ($Pfx) {
  $signtool = SdkTool 'signtool.exe'
  $sign = @('sign', '/fd', 'SHA256', '/f', (Resolve-Path $Pfx).Path)
  if ($env:MSSTORE_PFX_PASSWORD) { $sign += @('/p', $env:MSSTORE_PFX_PASSWORD) }
  Run $signtool @sign $bundle
}

Write-Host "packed $bundle"
