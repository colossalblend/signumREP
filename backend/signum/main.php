<?php
declare(strict_types=1);

ini_set('display_errors', '1');
ini_set('display_startup_errors', '1');
error_reporting(E_ALL);

require_once __DIR__ . '/common.php';

signum_handle_options();

$client = signum_client_context();
signum_require_min_client($client);

$category = trim((string)($_GET['category'] ?? ''));
$apps = signum_active_catalog_apps($client['vers']);

// Group chips come from the apps this client can actually see, so a title held
// back by min_client_version does not leave an empty tab behind. Collected
// before the ?category= narrowing below, which must not shrink the tab bar.
$visibleCategories = [];
foreach ($apps as $visibleApp) {
    $visibleCategories[(string)($visibleApp['category'] ?? 'app')] = true;
}
$groups = array_values(array_filter(
    signum_catalog_groups(),
    static fn(array $group): bool => isset($visibleCategories[(string)$group['id']])
));

if ($category !== '') {
    $apps = array_values(array_filter(
        $apps,
        static fn(array $app): bool => ($app['category'] ?? 'app') === $category
    ));
}

signum_json_response([
    'ok' => true,
    'api_version' => SIGNUM_API_VERSION,
    'min_client_version' => signum_ota_min_client_version((string)($client['platform'] ?? 'windows')),
    'server_time' => gmdate('c'),
    'client' => $client,
    'count' => count($apps),
    'groups' => $groups,
    'banners' => signum_active_catalog_banners($client['vers'], $client['platform']),
    'apps' => array_map('signum_catalog_summary', $apps),
]);
