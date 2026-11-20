[CmdletBinding()]
param(
    [string]$RepositoryRoot = 'D:\Signum-v1',
    [string]$ApiRoot = 'C:\Users\User\Downloads\web\api.appbank.pw\signum',
    [string]$BackupRoot = 'D:\Signum-backups'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$timestamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$backupName = "signum-source-before-comment-cleanup-$timestamp"
$stageRoot = Join-Path $BackupRoot "$backupName.stage"
$archivePath = Join-Path $BackupRoot "$backupName.zip"
$manifestPath = Join-Path $BackupRoot "$backupName.manifest.json"
$sha256Path = Join-Path $BackupRoot "$backupName.zip.sha256"

$extensions = @(
    '.rs', '.ts', '.tsx', '.js', '.jsx', '.mjs', '.c', '.cc', '.cpp', '.h', '.hh', '.hpp',
    '.py', '.php', '.kt', '.kts', '.java', '.cs', '.swift', '.sh', '.ps1', '.css', '.scss',
    '.html', '.htm', '.vue', '.sql', '.toml', '.json', '.jsonc', '.yaml', '.yml', '.md',
    '.txt', '.gradle', '.properties', '.xml', '.plist', '.lock'
)
$specialNames = @('Cargo.toml', 'Cargo.lock', 'package.json', 'package-lock.json', 'pnpm-lock.yaml', 'yarn.lock', 'Makefile', 'Dockerfile', '.gitignore', '.npmrc', '.prettierrc', '.eslintrc', '.editorconfig')
$excludedDirectories = @('node_modules', 'target', 'build', 'dist', 'frontend-dist', '.git', '.cache', '__pycache__', '.gradle')

function Test-SourceFile {
    param(
        [Parameter(Mandatory = $true)][System.IO.FileInfo]$File,
        [Parameter(Mandatory = $true)][string]$Root
    )

    $relative = $File.FullName.Substring($Root.Length).TrimStart('\', '/')
    $parts = $relative -split '[\\/]'
    if ($parts | Where-Object { $excludedDirectories -contains $_ }) {
        return $false
    }
    if ($specialNames -contains $File.Name) {
        return $true
    }
    return $extensions -contains $File.Extension.ToLowerInvariant()
}

function Copy-SourceTree {
    param(
        [Parameter(Mandatory = $true)][string]$SourceRoot,
        [Parameter(Mandatory = $true)][string]$StageRoot,
        [Parameter(Mandatory = $true)][string]$Label
    )

    if (-not (Test-Path -LiteralPath $SourceRoot -PathType Container)) {
        throw "Source root not found: $SourceRoot"
    }

    $records = New-Object System.Collections.Generic.List[object]
    Get-ChildItem -LiteralPath $SourceRoot -Recurse -File | Where-Object {
        Test-SourceFile -File $_ -Root $SourceRoot
    } | ForEach-Object {
        $relative = $_.FullName.Substring($SourceRoot.Length).TrimStart('\', '/')
        $destination = Join-Path $StageRoot $relative
        $parent = Split-Path -Parent $destination
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
        Copy-Item -LiteralPath $_.FullName -Destination $destination -Force
        $records.Add([ordered]@{
            area = $Label
            relative_path = $relative.Replace('\', '/')
            size = [Int64]$_.Length
            sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        })
    }
    return $records
}

New-Item -ItemType Directory -Path $BackupRoot -Force | Out-Null
Remove-Item -LiteralPath $stageRoot -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $archivePath -Force -ErrorAction SilentlyContinue

$repositoryRecords = Copy-SourceTree -SourceRoot $RepositoryRoot -StageRoot (Join-Path $stageRoot 'Signum-v1') -Label 'repository'
$apiRecords = Copy-SourceTree -SourceRoot $ApiRoot -StageRoot (Join-Path $stageRoot 'api.appbank.pw/signum') -Label 'api'
$records = @($repositoryRecords) + @($apiRecords)

$manifest = [ordered]@{
    created_at_utc = [DateTime]::UtcNow.ToString('o')
    repository_root = $RepositoryRoot
    api_root = $ApiRoot
    file_count = @($records).Count
    files = $records
}
$manifest | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $stageRoot 'backup-manifest.json') -Encoding UTF8
Compress-Archive -Path (Join-Path $stageRoot '*') -DestinationPath $archivePath -CompressionLevel Optimal
$archiveHash = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
$manifest | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $manifestPath -Encoding UTF8
"$archiveHash  $([System.IO.Path]::GetFileName($archivePath))" | Set-Content -LiteralPath $sha256Path -Encoding ASCII

Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [System.IO.Compression.ZipFile]::OpenRead($archivePath)
$zipEntryCount = $zip.Entries.Count
$zip.Dispose()
if ($zipEntryCount -lt @($records).Count) {
    throw "Archive validation failed: entries=$zipEntryCount expected-at-least=$(@($records).Count)"
}

Remove-Item -LiteralPath $stageRoot -Recurse -Force
Write-Output "archive=$archivePath"
Write-Output "manifest=$manifestPath"
Write-Output "sha256=$sha256Path"
Write-Output "source_files=$(@($records).Count)"
Write-Output "zip_entries=$zipEntryCount"
Write-Output "archive_sha256=$archiveHash"
