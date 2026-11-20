<?php
declare(strict_types=1);

ini_set('display_errors', '1');
ini_set('display_startup_errors', '1');
error_reporting(E_ALL);

require_once __DIR__ . '/common.php';

signum_handle_options();

$client = signum_client_context();
signum_require_min_client($client);

$iosVersion = trim((string)($_GET['ios'] ?? ''));
$ios = $iosVersion !== '' ? $iosVersion : null;

/**
 * Batch form: ?ids=a,b,c
 *
 * The client used to ask for one app per request inside a sequential loop, so a
 * full catalogue scan cost one round trip per app — with ~210 apps and ~0.28s of
 * edge latency apiece that is about a minute before anything is shown. The
 * payload was never the problem; the request count was.
 */
$idsParam = trim((string)($_GET['ids'] ?? ''));
if ($idsParam !== '') {
    $ids = array_values(array_unique(array_filter(
        array_map('trim', explode(',', $idsParam)),
        static fn(string $v): bool => $v !== ''
    )));
    // Bound the fan-out so one request cannot walk the whole catalogue twice.
    $ids = array_slice($ids, 0, 400);

    $data = [];
    $missing = [];
    foreach ($ids as $id) {
        $app = signum_get_app_or_null($id, $client['vers']);
        if ($app === null) {
            $missing[] = $id;
            continue;
        }
        $payload = signum_enrich_app($app, $ios);
        $payload['installable'] = !empty($payload['versions']);
        $data[$id] = $payload;
    }

    signum_json_response([
        'ok' => true,
        'api_version' => SIGNUM_API_VERSION,
        'min_client_version' => signum_ota_min_client_version((string)($client['platform'] ?? 'windows')),
        'count' => count($data),
        'missing' => $missing,
        'data' => $data,
    ]);
}

$id = trim((string)($_GET['id'] ?? ''));
$app = signum_get_app_or_null($id, $client['vers']);

if ($app === null) {
    signum_json_response([
        'ok' => false,
        'error' => 'Application not found',
        'id' => $id,
    ], 404);
}

$payload = signum_enrich_app($app, $ios);
$payload['installable'] = !empty($payload['versions']);

signum_json_response([
    'ok' => true,
    'api_version' => SIGNUM_API_VERSION,
    'min_client_version' => signum_ota_min_client_version((string)($client['platform'] ?? 'windows')),
    'data' => $payload,
]);
