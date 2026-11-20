<?php
declare(strict_types=1);

/**
 * Актуальные версии Signum для блока загрузок.
 *
 * Версии и ссылки берутся из того же releases.php, что использует приложение.
 * Большие файлы отдаёт веб-сервер напрямую, с поддержкой Range и докачки.
 */

header('Content-Type: application/json; charset=UTF-8');
header('Cache-Control: no-store, no-cache, must-revalidate, max-age=0');
header('Pragma: no-cache');

require_once __DIR__ . '/release-data.php';
try {
    $platforms = signum_site_releases();
    if ($platforms === []) { throw new RuntimeException('Release unavailable'); }
    echo json_encode(['ok' => true, 'generated_at' => gmdate('c'), 'platforms' => $platforms], JSON_UNESCAPED_UNICODE | JSON_THROW_ON_ERROR);
} catch (Throwable $error) {
    http_response_code(503);
    echo json_encode(['ok' => false, 'platforms' => new stdClass()]);
}
