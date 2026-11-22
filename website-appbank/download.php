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
    $path = signum_site_release_path($platform, $format === 'deb');
    if ($path === null) {
        throw new RuntimeException('Release unavailable');
    }
    $file = basename($path);

    // Prefer serving from this host: /releases/ is a link to the release
    // storage, so the browser stays on appbank.pw and the web server handles
    // Content-Length, Range/206 and resumed downloads without PHP limits.
    $localFile = __DIR__ . '/releases/' . $file;
    if (is_file($localFile) && filesize($localFile) > 0) {
        header('Location: /signum/releases/' . rawurlencode($file), true, 302);
        header('Content-Length: 0');
        exit;
    }

    // Fallback: the API host serves the same versioned file.
    header('Location: https://api.appbank.pw/signum' . $path, true, 302);
    header('Content-Length: 0');
} catch (Throwable $error) {
    http_response_code(503);
    header('Retry-After: 30');
    echo 'Не удалось подготовить загрузку. Попробуйте ещё раз чуть позже.';
}
