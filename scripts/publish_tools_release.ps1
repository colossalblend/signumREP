[CmdletBinding()]
param(
    [ValidatePattern('^\d+(\.\d+){0,3}([+-][A-Za-z0-9.]+)?$')]
    [string]$IpatoolVersion = '3.9.9.2',

    [ValidatePattern('^\d+(\.\d+){0,3}([+-][A-Za-z0-9.]+)?$')]
    [string]$AnisetteVersion = '3.0.0-legacy',

    # ipatool-cpp (Sorvigolova) — engine v5, SAP signer compiled in.
    [ValidatePattern('^\d+(\.\d+){0,3}([+-][A-Za-z0-9.]+)?$')]
    [string]$IpatoolCppVersion = '1.2.0',

    [ValidatePattern('^\d+(\.\d+){0,3}([+-][A-Za-z0-9.]+)?$')]
    [string]$IpatoolWindowsCppVersion = '1.3.0',

    [string]$MacOsAnisetteX64,
    [string]$MacOsAnisetteArm64,
    [string]$MacOsSapsignerX64,
    [string]$MacOsSapsignerArm64,
    [string]$LinuxAnisetteX64,
    [string]$LinuxAnisetteArm64
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$releaseRoot = Join-Path $repoRoot 'backend\signum\files\tools'

function Copy-ToolAsset {
    param(
        [Parameter(Mandatory = $true)][System.Collections.IDictionary]$Target,
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Version,
        [Parameter(Mandatory = $true)][string]$TargetDirectory
    )

    if (-not (Test-Path -LiteralPath $Source -PathType Leaf)) {
        throw "Tool artifact not found: $Source"
    }

    $extension = [IO.Path]::GetExtension($Source)
    $fileName = "$Name-$Version$extension"
    $destination = Join-Path $TargetDirectory $fileName
    Copy-Item -LiteralPath $Source -Destination $destination -Force

    $hash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
    $Target[$Name] = [ordered]@{
        version = $Version
        file = $fileName
        sha256 = $hash
        size = [Int64](Get-Item -LiteralPath $destination).Length
    }
}

function Add-OptionalAsset {
    param(
        [Parameter(Mandatory = $true)][System.Collections.IDictionary]$Target,
        [Parameter(Mandatory = $true)][string]$Name,
        [AllowEmptyString()][string]$Source,
        [Parameter(Mandatory = $true)][string]$Version,
        [Parameter(Mandatory = $true)][string]$TargetDirectory
    )

    if ([string]::IsNullOrWhiteSpace($Source)) {
        return
    }
    Copy-ToolAsset -Target $Target -Name $Name -Source $Source -Version $Version -TargetDirectory $TargetDirectory
}

$targets = [ordered]@{
    'windows-x64' = [ordered]@{}
    'macos-x64' = [ordered]@{}
    'macos-arm64' = [ordered]@{}
    'linux-x64' = [ordered]@{}
    'linux-arm64' = [ordered]@{}
}

foreach ($target in $targets.Keys) {
    $targetDir = Join-Path $releaseRoot $target
    New-Item -ItemType Directory -Path $targetDir -Force | Out-Null
    Get-ChildItem -LiteralPath $targetDir -File -ErrorAction SilentlyContinue | Remove-Item -Force
}

Copy-ToolAsset -Target $targets['windows-x64'] -Name 'ipatool-v5' -Source (Join-Path $repoRoot 'apple-tools\windows-x64\v5\ipatool.exe') -Version $IpatoolWindowsCppVersion -TargetDirectory (Join-Path $releaseRoot 'windows-x64')
Copy-ToolAsset -Target $targets['windows-x64'] -Name 'ipatool-v2' -Source (Join-Path $repoRoot 'apple-tools\windows-x64\v2\ipatool.exe') -Version $IpatoolVersion -TargetDirectory (Join-Path $releaseRoot 'windows-x64')
Copy-ToolAsset -Target $targets['windows-x64'] -Name 'ipatool-v3' -Source (Join-Path $repoRoot 'apple-tools\windows-x64\v3-legacy\ipatool.exe') -Version $AnisetteVersion -TargetDirectory (Join-Path $releaseRoot 'windows-x64')
Copy-ToolAsset -Target $targets['windows-x64'] -Name 'anisette' -Source (Join-Path $repoRoot 'apple-tools\windows-x64\v3-legacy\anisette.exe') -Version $AnisetteVersion -TargetDirectory (Join-Path $releaseRoot 'windows-x64')

Copy-ToolAsset -Target $targets['macos-x64'] -Name 'ipatool-v5' -Source (Join-Path $repoRoot 'apple-tools\macos-x64\v5\ipatool') -Version $IpatoolCppVersion -TargetDirectory (Join-Path $releaseRoot 'macos-x64')
Copy-ToolAsset -Target $targets['macos-x64'] -Name 'ipatool-v2' -Source (Join-Path $repoRoot 'apple-tools\macos-x64\v2\ipatool') -Version $IpatoolVersion -TargetDirectory (Join-Path $releaseRoot 'macos-x64')
Copy-ToolAsset -Target $targets['macos-x64'] -Name 'ipatool-v3' -Source (Join-Path $repoRoot 'apple-tools\macos-x64\v3-legacy\ipatool') -Version $AnisetteVersion -TargetDirectory (Join-Path $releaseRoot 'macos-x64')
Add-OptionalAsset -Target $targets['macos-x64'] -Name 'anisette' -Source $MacOsAnisetteX64 -Version $AnisetteVersion -TargetDirectory (Join-Path $releaseRoot 'macos-x64')
Add-OptionalAsset -Target $targets['macos-x64'] -Name 'sapsigner' -Source $MacOsSapsignerX64 -Version $IpatoolVersion -TargetDirectory (Join-Path $releaseRoot 'macos-x64')

Copy-ToolAsset -Target $targets['macos-arm64'] -Name 'ipatool-v5' -Source (Join-Path $repoRoot 'apple-tools\macos-arm64\v5\ipatool') -Version $IpatoolCppVersion -TargetDirectory (Join-Path $releaseRoot 'macos-arm64')
Copy-ToolAsset -Target $targets['macos-arm64'] -Name 'ipatool-v2' -Source (Join-Path $repoRoot 'apple-tools\macos-arm64\v2\ipatool') -Version $IpatoolVersion -TargetDirectory (Join-Path $releaseRoot 'macos-arm64')
Copy-ToolAsset -Target $targets['macos-arm64'] -Name 'ipatool-v3' -Source (Join-Path $repoRoot 'apple-tools\macos-arm64\v3-legacy\ipatool') -Version $AnisetteVersion -TargetDirectory (Join-Path $releaseRoot 'macos-arm64')
Add-OptionalAsset -Target $targets['macos-arm64'] -Name 'anisette' -Source $MacOsAnisetteArm64 -Version $AnisetteVersion -TargetDirectory (Join-Path $releaseRoot 'macos-arm64')
Add-OptionalAsset -Target $targets['macos-arm64'] -Name 'sapsigner' -Source $MacOsSapsignerArm64 -Version $IpatoolVersion -TargetDirectory (Join-Path $releaseRoot 'macos-arm64')

Copy-ToolAsset -Target $targets['linux-x64'] -Name 'ipatool-v5' -Source (Join-Path $repoRoot 'linux_x64\ipatool-v5') -Version $IpatoolCppVersion -TargetDirectory (Join-Path $releaseRoot 'linux-x64')
Copy-ToolAsset -Target $targets['linux-x64'] -Name 'ipatool-v2' -Source (Join-Path $repoRoot 'linux_x64\ipatool') -Version $IpatoolVersion -TargetDirectory (Join-Path $releaseRoot 'linux-x64')
Add-OptionalAsset -Target $targets['linux-x64'] -Name 'anisette' -Source $LinuxAnisetteX64 -Version $AnisetteVersion -TargetDirectory (Join-Path $releaseRoot 'linux-x64')

Copy-ToolAsset -Target $targets['linux-arm64'] -Name 'ipatool-v2' -Source (Join-Path $repoRoot 'linux_arm64\ipatool') -Version $IpatoolVersion -TargetDirectory (Join-Path $releaseRoot 'linux-arm64')
Add-OptionalAsset -Target $targets['linux-arm64'] -Name 'anisette' -Source $LinuxAnisetteArm64 -Version $AnisetteVersion -TargetDirectory (Join-Path $releaseRoot 'linux-arm64')

$manifest = [ordered]@{
    schema_version = 1
    generated_at = [DateTime]::UtcNow.ToString('o')
    targets = $targets
}

New-Item -ItemType Directory -Path $releaseRoot -Force | Out-Null
$manifestPath = Join-Path $releaseRoot 'manifest.json'
$manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifestPath -Encoding UTF8
Write-Host "Ready: $manifestPath"
Write-Host 'Deploy backend/signum, including tools.php and files/tools/, to the API server.'
