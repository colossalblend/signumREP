Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$apiRoot = 'C:\Users\User\Downloads\web\api.appbank.pw\signum'
$commonPath = Join-Path $apiRoot 'common.php'
$mainPath = Join-Path $apiRoot 'main.php'
$utf8 = [System.Text.UTF8Encoding]::new($false)

function Read-Utf8([string]$path) {
    return [System.IO.File]::ReadAllText($path, [System.Text.UTF8Encoding]::new($false, $true))
}

function Write-Utf8([string]$path, [string]$content) {
    [System.IO.File]::WriteAllText($path, $content, $utf8)
}

function Replace-Region([string]$source, [string]$startMarker, [string]$endMarker, [string]$replacement) {
    $start = $source.IndexOf($startMarker, [System.StringComparison]::Ordinal)
    if ($start -lt 0) { throw "Start marker not found: $startMarker" }
    $end = $source.IndexOf($endMarker, $start, [System.StringComparison]::Ordinal)
    if ($end -lt 0) { throw "End marker not found: $endMarker" }
    return $source.Substring(0, $start) + $replacement.TrimEnd() + "`r`n" + $source.Substring($end)
}

if (-not (Test-Path $commonPath) -or -not (Test-Path $mainPath)) {
    throw 'Signum API source files were not found.'
}

$common = Read-Utf8 $commonPath
if ($common.Contains("'app_store_id' => '6785551921'")) {
    throw 'K8CHEN PRO is already present in the catalog.'
}

$bannerFunctions = @'
/**
 * Text banners above the catalog. A banner may target all clients or a version
 * range. The optional conditions are configuration-only and are not exposed to
 * the desktop UI:
 *
 * - min_client_version: shown from this version, inclusive;
 * - max_client_version: shown through this version, inclusive;
 * - client_versions: optional exact-version allow-list;
 * - platforms: optional allow-list (windows, linux, macos, android).
 */
function signum_catalog_banners(): array
{
    return [
        [
            'id' => 'tbank-new-example',
            'active' => false,
            'title' => 'Новое приложение Т-Банка в каталоге',
            'text' => 'Новую версию можно выбрать в карточке Т-Банка.',
            'style' => 'blue',
            'app_id' => 'tbank',
            'action_label' => 'Открыть каталог',
            'priority' => 100,
            // Example: show only client versions 1.1.0 through 1.1.9.
            'min_client_version' => '1.1.0',
            'max_client_version' => '1.1.9',
            // Example alternatives: 'client_versions' => ['1.1.0', '1.1.1'],
            // or 'platforms' => ['windows', 'linux'].
        ],
    ];
}
function signum_banner_visible_to_client(array $banner, ?string $clientVersion = null, ?string $platform = null): bool
{
    $clientVersion = trim((string)($clientVersion ?? ''));
    $platform = signum_normalize_update_platform($platform ?? 'windows');
    $minVersion = trim((string)($banner['min_client_version'] ?? ''));
    $maxVersion = trim((string)($banner['max_client_version'] ?? ''));
    $exactVersions = $banner['client_versions'] ?? [];
    $platforms = $banner['platforms'] ?? [];

    if (($minVersion !== '' || $maxVersion !== '' || (is_array($exactVersions) && $exactVersions !== [])) && $clientVersion === '') {
        return false;
    }
    if ($minVersion !== '' && signum_compare_builds($clientVersion, $minVersion) < 0) {
        return false;
    }
    if ($maxVersion !== '' && signum_compare_builds($clientVersion, $maxVersion) > 0) {
        return false;
    }
    if (is_array($exactVersions) && $exactVersions !== []) {
        $allowed = array_map(static fn($version): string => trim((string)$version), $exactVersions);
        if (!in_array($clientVersion, $allowed, true)) {
            return false;
        }
    }
    if (is_array($platforms) && $platforms !== []) {
        $allowedPlatforms = array_map('signum_normalize_update_platform', $platforms);
        if (!in_array($platform, $allowedPlatforms, true)) {
            return false;
        }
    }
    return true;
}
function signum_active_catalog_banners(?string $clientVersion = null, ?string $platform = null): array
{
    $clientVersion = $clientVersion ?? trim((string)($_GET['vers'] ?? ''));
    $platform = $platform ?? trim((string)($_GET['platform'] ?? $_GET['os'] ?? 'windows'));
    $banners = array_values(array_filter(
        signum_catalog_banners(),
        static fn(array $banner): bool => !empty($banner['active'])
            && signum_banner_visible_to_client($banner, $clientVersion, $platform)
    ));
    usort($banners, static fn(array $left, array $right): int => ((int)($right['priority'] ?? 0)) <=> ((int)($left['priority'] ?? 0)));
    return array_values(array_map(static function (array $banner): array {
        return [
            'id' => (string)($banner['id'] ?? ''),
            'title' => (string)($banner['title'] ?? ''),
            'text' => (string)($banner['text'] ?? ''),
            'style' => (string)($banner['style'] ?? 'blue'),
            'app_id' => isset($banner['app_id']) ? (string)$banner['app_id'] : null,
            'action_label' => isset($banner['action_label']) ? (string)$banner['action_label'] : null,
            'priority' => (int)($banner['priority'] ?? 0),
        ];
    }, $banners));
}
'@
$common = Replace-Region $common 'function signum_catalog_banners(): array' 'function signum_catalog_apps(): array' $bannerFunctions

$airPilot = @'
                [
                    'title' => 'Air Pilot Log',
                    'description' => '',
                    'version' => '7.39.1',
                    'bundle_id' => 'balloon.pilot.log',
                    'app_store_id' => '6778332772',
                    'icon_url' => 'https://is2-ssl.mzstatic.com/image/thumb/Purple221/v4/95/cd/ac/95cdac26-6510-ea41-9033-28bcf05ea456/LogoMainKitchenBrief-0-0-1x_U007epad-0-1-85-220.png/0x300h.png',
                    'min_ios' => '16.0',
                    'is_latest' => true,
                    'source' => 'removed',
                    'in_store' => false,
                ],
'@
$airPilotTitle = "                    'title' => 'Air Pilot Log',"
$airPilotTitleIndex = $common.IndexOf($airPilotTitle, [System.StringComparison]::Ordinal)
if ($airPilotTitleIndex -lt 0) {
    throw 'Air Pilot Log title anchor was not found.'
}
$airPilotStart = $common.LastIndexOf("                [", $airPilotTitleIndex, [System.StringComparison]::Ordinal)
$airPilotEnd = $common.IndexOf("                ],", $airPilotTitleIndex, [System.StringComparison]::Ordinal)
if ($airPilotStart -lt 0 -or $airPilotEnd -lt 0) {
    throw 'Air Pilot Log entry boundaries were not found.'
}
$airPilotEnd += "                ],".Length
$airPilotEntry = $common.Substring($airPilotStart, $airPilotEnd - $airPilotStart)
$k8chen = @'
                [
                    'title' => 'K8CHEN PRO',
                    'description' => 'Приложение Т-Банка.',
                    'version' => '8.0.1',
                    'bundle_id' => 'com.kitchen.brief',
                    'app_store_id' => '6785551921',
                    'icon_url' => 'https://is1-ssl.mzstatic.com/image/thumb/Purple221/v4/95/cd/ac/95cdac26-6510-ea41-9033-28bcf05ea456/LogoMainKitchenBrief-0-0-1x_U007epad-0-1-85-220.png/0x500h.png',
                    'min_ios' => '16.0',
                    'is_latest' => true,
                    'source' => 'removed',
                    'in_store' => false,
                    'active' => true,
                ],
'@
$airPilotUpdated = $airPilotEntry.Replace("'is_latest' => true", "'is_latest' => false")
$common = $common.Substring(0, $airPilotStart) + $k8chen + $airPilotUpdated + $common.Substring($airPilotEnd)
Write-Utf8 $commonPath $common

$main = Read-Utf8 $mainPath
$oldBannerCall = "'banners' => signum_active_catalog_banners(),"
$newBannerCall = "'banners' => signum_active_catalog_banners(`$client['vers'], `$client['platform']),"
if (-not $main.Contains($oldBannerCall)) {
    throw 'Catalog banner call was not found in main.php.'
}
$main = $main.Replace($oldBannerCall, $newBannerCall)
Write-Utf8 $mainPath $main

Write-Output 'Versioned banners and K8CHEN PRO were added to the API.'
