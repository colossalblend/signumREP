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
    'deb_download_url' => $hasUpdate ? ($release['deb_download_url'] ?? null) : null,
    'deb_sha256' => $hasUpdate ? ($release['deb_sha256'] ?? null) : null,
    'client' => $client,
]);