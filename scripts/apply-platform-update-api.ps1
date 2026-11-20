Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$apiRoot = 'C:\Users\User\Downloads\web\api.appbank.pw\signum'
$commonPath = Join-Path $apiRoot 'common.php'
$otaPath = Join-Path $apiRoot 'ota.php'

function Read-Utf8([string]$path) {
    return [System.IO.File]::ReadAllText($path)
}

function Write-Utf8NoBom([string]$path, [string]$content) {
    [System.IO.File]::WriteAllText($path, $content, [System.Text.UTF8Encoding]::new($false))
}

function Replace-Region([string]$source, [string]$startMarker, [string]$endMarker, [string]$replacement) {
    $start = $source.IndexOf($startMarker, [System.StringComparison]::Ordinal)
    if ($start -lt 0) { throw "Start marker not found: $startMarker" }
    $end = $source.IndexOf($endMarker, $start, [System.StringComparison]::Ordinal)
    if ($end -lt 0) { throw "End marker not found: $endMarker" }
    return $source.Substring(0, $start) + $replacement.TrimEnd() + "`r`n" + $source.Substring($end)
}

if (-not (Test-Path $commonPath) -or -not (Test-Path $otaPath)) {
    throw 'Signum API source files were not found.'
}

Copy-Item -LiteralPath $commonPath -Destination "$commonPath.platform-update.bak" -Force
Copy-Item -LiteralPath $otaPath -Destination "$otaPath.platform-update.bak" -Force

$common = Read-Utf8 $commonPath
$common = [regex]::Replace(
    $common,
    "(?m)^const SIGNUM_MIN_CLIENT_VERSION = '1\.0\.0';\r?\n",
    "const SIGNUM_MIN_CLIENT_VERSION = '1.0.0';`r`nconst SIGNUM_RELEASE_MANIFEST = __DIR__ . '/releases.php';`r`n",
    1
)
$common = [regex]::Replace($common, "(?m)^const SIGNUM_LATEST_VERSION = .*\r?\n", '', 1)
$common = [regex]::Replace($common, "(?m)^const SIGNUM_LATEST_BUILD = .*\r?\n", '', 1)

$clientContext = @'
function signum_normalize_update_platform(?string $value): string
{
    $value = strtolower(trim((string)$value));
    $aliases = [
        'windows' => 'windows',
        'win' => 'windows',
        'desktop' => 'windows',
        'linux' => 'linux',
        'macos' => 'macos',
        'mac' => 'macos',
        'darwin' => 'macos',
        'android' => 'android',
    ];
    return $aliases[$value] ?? 'unknown';
}
function signum_client_context(): array
{
    $requestedPlatform = $_GET['platform'] ?? $_GET['os'] ?? 'windows';
    return [
        'vers' => trim((string)($_GET['vers'] ?? '')),
        'build' => trim((string)($_GET['build'] ?? '')),
        'devkey' => trim((string)($_GET['devkey'] ?? '')),
        // platform is canonical; os remains a compatibility alias for callers.
        'platform' => signum_normalize_update_platform((string)$requestedPlatform),
    ];
}
'@
$common = Replace-Region $common 'function signum_client_context(): array' 'function signum_compare_builds' $clientContext

$releaseFunctions = @'
function signum_ota_releases(): array
{
    if (!is_file(SIGNUM_RELEASE_MANIFEST)) {
        return [];
    }
    $releases = require SIGNUM_RELEASE_MANIFEST;
    return is_array($releases) ? $releases : [];
}
function signum_ota_release(?string $platform = null): ?array
{
    $platform = signum_normalize_update_platform($platform ?? 'windows');
    $release = signum_ota_releases()[$platform] ?? null;
    if (!is_array($release)) {
        return null;
    }
    $version = trim((string)($release['version'] ?? ''));
    $downloadPath = trim((string)($release['download_url'] ?? ''));
    if ($version === '' || $downloadPath === '' || !str_starts_with($downloadPath, '/releases/')) {
        return null;
    }
    $sha256 = strtolower(trim((string)($release['sha256'] ?? '')));
    return [
        'platform' => $platform,
        'version' => $version,
        'build' => max(0, (int)($release['build'] ?? 0)),
        'mandatory' => !empty($release['mandatory']),
        'notes' => trim((string)($release['notes'] ?? '')),
        'download_url' => signum_base_url() . $downloadPath,
        'sha256' => preg_match('/^[a-f0-9]{64}$/', $sha256) ? $sha256 : null,
        'published_at' => trim((string)($release['published_at'] ?? '')),
        'min_client_version' => trim((string)($release['min_client_version'] ?? SIGNUM_MIN_CLIENT_VERSION)),
    ];
}
function signum_ota_min_client_version(?string $platform = null): ?string
{
    $release = signum_ota_release($platform);
    $minimum = trim((string)($release['min_client_version'] ?? ''));
    return $minimum !== '' ? $minimum : null;
}
function signum_has_ota_update(string $clientVersion, int $clientBuild, ?array $release = null): bool
{
    if ($release === null) {
        return false;
    }
    if ($clientVersion === '') {
        return true;
    }
    if (signum_compare_builds($clientVersion, (string)$release['version']) < 0) {
        return true;
    }
    return $clientBuild < (int)$release['build'];
}
/**
 * Whether this platform has a managed release and the client is new enough to use the API.
 */
function signum_client_is_supported(?string $clientVersion, ?string $platform = null): bool
{
    $minimum = signum_ota_min_client_version($platform);
    if ($minimum === null) {
        return true;
    }
    $version = trim((string)$clientVersion);
    return $version !== '' && signum_compare_builds($version, $minimum) >= 0;
}
/**
 * Hard gate for API endpoints. Platforms without a published managed release
 * are never blocked by another platform's minimum version.
 */
function signum_require_min_client(?array $client = null): void
{
    $client = $client ?? signum_client_context();
    $platform = signum_normalize_update_platform((string)($client['platform'] ?? 'windows'));
    $version = (string)($client['vers'] ?? '');
    if (signum_client_is_supported($version, $platform)) {
        return;
    }
    $release = signum_ota_release($platform);
    $minimum = signum_ota_min_client_version($platform);
    signum_json_response([
        'ok' => false,
        'error' => 'client_outdated',
        'message' => 'Эта версия Signum устарела. Обновите приложение до версии ' . $minimum . ' или новее.',
        'min_client_version' => $minimum,
        'latest_version' => $release['version'] ?? null,
        'download_url' => $release['download_url'] ?? null,
        'sha256' => $release['sha256'] ?? null,
        'platform' => $platform,
    ], 426);
}
'@
$common = Replace-Region $common 'function signum_ota_release(): array' 'function signum_auto_purchase_enabled(): bool' $releaseFunctions
Write-Utf8NoBom $commonPath $common

$ota = @'
<?php
declare(strict_types=1);
ini_set('display_errors', '1');
ini_set('display_startup_errors', '1');
error_reporting(E_ALL);
require_once __DIR__ . '/common.php';
signum_handle_options();
$client = signum_client_context();
$platform = $client['platform'];
$clientVersion = $client['vers'] !== '' ? $client['vers'] : '0.0.0';
$clientBuild = (int)($client['build'] !== '' ? $client['build'] : 0);
$release = signum_ota_release($platform);
$clientSupported = signum_client_is_supported($clientVersion, $platform);
$hasUpdate = signum_has_ota_update($clientVersion, $clientBuild, $release);
signum_json_response([
    'ok' => true,
    'api_version' => SIGNUM_API_VERSION,
    'platform' => $platform,
    'release_available' => $release !== null,
    'client_supported' => $clientSupported,
    'min_client_version' => signum_ota_min_client_version($platform),
    'has_update' => $hasUpdate,
    'version' => $release['version'] ?? null,
    'build' => isset($release['build']) ? (string)$release['build'] : null,
    'mandatory' => $release !== null && !empty($release['mandatory']),
    'notes' => $release['notes'] ?? null,
    'published_at' => $release['published_at'] ?? null,
    'download_url' => $hasUpdate ? ($release['download_url'] ?? null) : null,
    'sha256' => $hasUpdate ? ($release['sha256'] ?? null) : null,
    'client' => $client,
]);
'@
Write-Utf8NoBom $otaPath $ota

foreach ($endpoint in @('main.php', 'info.php', 'check.php')) {
    $path = Join-Path $apiRoot $endpoint
    $text = Read-Utf8 $path
    $text = $text.Replace("'min_client_version' => SIGNUM_MIN_CLIENT_VERSION,", "'min_client_version' => signum_ota_min_client_version((string)(`$client['platform'] ?? 'windows'))," )
    Write-Utf8NoBom $path $text
}

Write-Output 'Platform update API sources were updated.'
