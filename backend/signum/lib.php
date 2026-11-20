<?php
declare(strict_types=1);

/**
 * Выдача вспомогательных библиотек Signum.
 *
 * Здесь лежат только системные библиотеки Apple из публичного пакета
 * обновления macOS — одинаковые для всех и не содержащие ничего пользовательского.
 * Нужны движку v5: без них он лезет за ними напрямую к Apple, а там, где Apple
 * недоступна, это молча отключает загрузку снятых с продажи сборок.
 *
 * GET lib.php?name=assets           — сам файл
 * GET lib.php?name=assets&meta=1    — размер и sha256, чтобы не качать зря
 */

require_once __DIR__ . '/common.php';

signum_handle_options();

/** Что разрешено отдавать. Белый список, а не произвольный путь. */
const SIGNUM_LIB_FILES = [
    'assets' => 'assets.bin',
];

$name = trim((string)($_GET['name'] ?? ''));
if (!isset(SIGNUM_LIB_FILES[$name])) {
    signum_json_response(['ok' => false, 'error' => 'unknown library'], 404);
}

$file = __DIR__ . '/lib/' . SIGNUM_LIB_FILES[$name];
if (!is_file($file)) {
    signum_json_response(['ok' => false, 'error' => 'not available'], 404);
}

$sha = '';
$shaFile = $file . '.sha256';
if (is_file($shaFile)) {
    $sha = trim((string)file_get_contents($shaFile));
}

if (!empty($_GET['meta'])) {
    signum_json_response([
        'ok' => true,
        'name' => $name,
        'size' => filesize($file),
        'sha256' => $sha,
    ]);
}

$size = filesize($file);
header('Content-Type: application/octet-stream');
header('Content-Length: ' . $size);
header('Content-Disposition: attachment; filename="' . SIGNUM_LIB_FILES[$name] . '"');
if ($sha !== '') {
    header('X-Signum-Sha256: ' . $sha);
}
// Файл не меняется между выпусками — пусть лежит в кеше и не тратит канал.
header('Cache-Control: public, max-age=604800');
header('Accept-Ranges: none');

// Отдаём потоком: 38 МБ целиком в память класть незачем.
$fh = fopen($file, 'rb');
if ($fh === false) {
    signum_json_response(['ok' => false, 'error' => 'read failed'], 500);
}
while (!feof($fh)) {
    echo fread($fh, 262144);
    flush();
}
fclose($fh);
