<?php
declare(strict_types=1);

require_once __DIR__ . '/release-data.php';
header('Cache-Control: no-store, no-cache, must-revalidate, max-age=0');
header('Pragma: no-cache');
$platform = (string)($_GET['platform'] ?? '');
$format = (string)($_GET['format'] ?? '');
if (!in_array($platform, ['windows', 'macos', 'linux', 'android'], true)
    || !in_array($format, ['', 'deb'], true)
    || ($format === 'deb' && $platform !== 'linux')) {
    http_response_code(400);
    exit('Выберите платформу для скачивания Signum.');
}
try {
    $release = signum_site_releases()[$platform] ?? [];
    $url = $release[$format === 'deb' ? 'deb_download_url' : 'download_url'] ?? null;
    if (!$url) {
        throw new RuntimeException('Release unavailable');
    }

    header('Location: ' . $url, true, 302);
    header('Content-Length: 0');
} catch (Throwable $error) {
    http_response_code(503);
    header('Retry-After: 30');
    echo 'Не удалось подготовить загрузку. Попробуйте ещё раз чуть позже.';
}
