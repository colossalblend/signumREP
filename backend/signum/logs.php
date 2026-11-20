<?php
declare(strict_types=1);

require_once __DIR__ . '/common.php';

// Двенадцать часов вместо часа: записей теперь больше, а разбирать их
// удобнее большими пачками, а не каждый час по горстке.
const SIGNUM_LOG_FLUSH_SECS = 43200;

if (($_SERVER['REQUEST_METHOD'] ?? 'GET') === 'OPTIONS') {
    http_response_code(200);
    header('Access-Control-Allow-Origin: *');
    header('Access-Control-Allow-Methods: POST, GET, OPTIONS');
    header('Access-Control-Allow-Headers: Content-Type');
    exit;
}

header('Access-Control-Allow-Origin: *');

$method = $_SERVER['REQUEST_METHOD'] ?? '';
if ($method === 'GET' && isset($_GET['flush'])) {
    $result = signum_telegram_flush(false);
    signum_json_response(['ok' => true, 'flush' => $result]);
}

if ($method !== 'POST') {
    signum_json_response(['ok' => false, 'error' => 'method'], 405);
}

$raw = file_get_contents('php://input') ?: '';
if (strlen($raw) > 1500000) {
    signum_json_response(['ok' => false, 'error' => 'too_large', 'message' => 'Отчёт слишком большой.'], 413);
}

$data = json_decode($raw, true);
if (!is_array($data)) {
    signum_json_response(['ok' => false, 'error' => 'invalid', 'message' => 'Не удалось прочитать отчёт.'], 400);
}

$kind = strtolower(trim((string)($data['kind'] ?? '')));
if ($kind !== 'auto' && $kind !== 'support') {
    $kind = trim((string)($data['message'] ?? '')) !== '' && trim((string)($data['log'] ?? '')) === ''
        ? 'auto'
        : 'support';
}
if ($kind === 'auto') {
    // Older clients may still post per-error reports. Acknowledge and discard:
    // only automatic support-session recordings belong in the 12-hour batch.
    signum_json_response(['ok' => true, 'delivered' => false, 'ignored' => true]);
}

$platform = strtolower(trim((string)($data['platform'] ?? 'unknown')));
$platform = preg_replace('/[^a-z0-9_\-]/', '', $platform) ?: 'unknown';
$source = strtolower(trim((string)($data['source'] ?? 'unknown')));
$source = substr(preg_replace('/[^a-z0-9_\-]/', '', $source) ?: 'unknown', 0, 32);
$version = substr(preg_replace('/[^0-9.]/', '', (string)($data['version'] ?? '')) ?: '0', 0, 24);
$build = substr(preg_replace('/[^0-9.]/', '', (string)($data['build'] ?? '')) ?: '0', 0, 24);

$message = signum_redact_client_log((string)($data['message'] ?? ''));
$log = signum_redact_client_log((string)($data['log'] ?? ''));
$message = substr($message, 0, 2000);
$log = substr($log, 0, 1200000);

// Запись, начатую приложением самостоятельно, в чат сразу не шлём: она
// приходит при каждом закрытии, и таких будет много. Копим отдельно и
// отдаём вместе с остальной пачкой.
$autoSession = $kind === 'support' && str_starts_with($source, 'auto');
$dir = signum_logs_root() . ($autoSession ? '/sessions' : '/support');
if (!is_dir($dir) && !mkdir($dir, 0750, true) && !is_dir($dir)) {
    signum_json_response(['ok' => false, 'error' => 'store', 'message' => 'Не удалось сохранить отчёт.'], 500);
}

$id = bin2hex(random_bytes(8));
$name = date('Ymd-His') . '-' . $kind . '-' . $platform . '-' . $id . '.json';
$path = $dir . '/' . $name;
$payload = [
    'received_at' => gmdate('c'),
    'kind' => $kind,
    'source' => $source,
    'platform' => $platform,
    'version' => $version,
    'build' => $build,
    'message' => $message,
    'log' => $log,
];
if ($kind === 'support') {
    $payload['ip'] = $_SERVER['REMOTE_ADDR'] ?? '';
}

if (file_put_contents($path, json_encode($payload, JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES), LOCK_EX) === false) {
    signum_json_response(['ok' => false, 'error' => 'store', 'message' => 'Не удалось сохранить отчёт.'], 500);
}

$delivered = true;
if ($autoSession) {
    // Ничего не отправляем — только проверяем, не пора ли слать пачку.
    signum_telegram_flush(true);
} elseif ($kind === 'support') {
    $delivered = signum_telegram_forward($payload, $path);
    if ($delivered) {
        @unlink($path);
    } else {
        // Не дошло — запись остаётся на диске, её ещё можно дослать.
        signum_prune_kept_reports($dir);
    }
} else {
    signum_telegram_flush(true);
}

signum_json_response(['ok' => true, 'delivered' => $delivered]);

function signum_logs_root(): string
{
    return __DIR__ . '/files/client-logs';
}

function signum_redact_client_log(string $text): string
{
    $text = preg_replace('/[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,}/i', '[email]', $text) ?? $text;
    $text = preg_replace('/(?i)(bearer\s+|token[=:]\s*|passwordtoken[=:]\s*|cookie[=:]\s*)[^\s,&]+/', '$1[hidden]', $text) ?? $text;
    $text = preg_replace('/\b\d{6}\b/', '[code]', $text) ?? $text;
    $text = preg_replace('/([A-Za-z]:\\\\Users\\\\)[^\\\\]+/i', '$1[user]', $text) ?? $text;
    $text = preg_replace('#(/(?:Users|home)/)[^/\s]+#', '$1[user]', $text) ?? $text;
    $text = preg_replace('/\b[0-9a-fA-F]{40}\b/', '[id]', $text) ?? $text;
    return $text;
}

function signum_telegram_config(): ?array
{
    $path = __DIR__ . '/tg.secret.php';
    if (!is_file($path)) {
        return null;
    }
    $cfg = require $path;
    if (!is_array($cfg)) {
        return null;
    }
    $token = trim((string)($cfg['token'] ?? ''));
    $chatId = trim((string)($cfg['chat_id'] ?? ''));
    $threadId = (int)($cfg['thread_id'] ?? 0);
    if ($token === '' || $chatId === '') {
        return null;
    }
    return [
        'token' => $token,
        'chat_id' => $chatId,
        'thread_id' => $threadId > 0 ? $threadId : null,
    ];
}

function signum_telegram_api(array $cfg, string $method, array $fields, int $timeout = 8): array
{
    $url = 'https://api.telegram.org/bot' . $cfg['token'] . '/' . $method;
    if (!empty($cfg['thread_id']) && !isset($fields['message_thread_id'])) {
        $fields['message_thread_id'] = $cfg['thread_id'];
    }
    $ch = curl_init($url);
    if ($ch === false) {
        return ['ok' => false];
    }
    curl_setopt_array($ch, [
        CURLOPT_POST => true,
        CURLOPT_POSTFIELDS => $fields,
        CURLOPT_RETURNTRANSFER => true,
        CURLOPT_CONNECTTIMEOUT => 4,
        CURLOPT_TIMEOUT => $timeout,
    ]);
    $raw = curl_exec($ch);
    curl_close($ch);
    $decoded = is_string($raw) ? json_decode($raw, true) : null;
    return is_array($decoded) ? $decoded : ['ok' => false];
}

function signum_telegram_send(array $cfg, string $method, array $fields, int $timeout = 8): array
{
    $result = signum_telegram_api($cfg, $method, $fields, $timeout);
    if (($result['ok'] ?? false) !== true && ctype_digit((string)$cfg['chat_id'])) {
        $cfg['chat_id'] = '-100' . $cfg['chat_id'];
        $result = signum_telegram_api($cfg, $method, $fields, $timeout);
    }
    return $result;
}

function signum_telegram_format(array $payload): string
{
    $kind = $payload['kind'] === 'support' ? 'запись для поддержки' : 'авто';
    $lines = [
        'Signum · ' . $kind,
        trim(($payload['platform'] ?? '?') . ' · ' . ($payload['version'] ?? '?') . ' (' . ($payload['build'] ?? '?') . ')'),
        'источник: ' . ($payload['source'] ?? '?'),
    ];
    $message = trim((string)($payload['message'] ?? ''));
    $log = trim((string)($payload['log'] ?? ''));
    if ($message !== '') {
        $lines[] = '';
        $lines[] = $message;
    }
    if ($log !== '') {
        $lines[] = '';
        $lines[] = $log;
    }
    $text = implode("\n", $lines);
    if (strlen($text) > 3900) {
        $text = substr($text, 0, 3900) . '…';
    }
    return $text;
}

/**
 * Отправить запись в Telegram. true — файл дошёл (или он не нужен).
 *
 * Раньше эта функция ничего не возвращала, а вызывающий код удалял запись с
 * диска сразу после вызова. Стоило Telegram не принять документ — и отчёт
 * пропадал навсегда: в чате оставался текст без файла, а на сервере не
 * оставалось ничего. Так и выглядела жалоба «пост пришёл, а файла нет».
 */
function signum_telegram_forward(array $payload, string $filePath): bool
{
    $cfg = signum_telegram_config();
    if ($cfg === null) {
        return false;
    }
    $text = signum_telegram_format($payload);
    $log = (string)($payload['log'] ?? '');
    // Запись для поддержки человек включает руками, чтобы показать проблему, —
    // её всегда отдаём файлом, даже короткую. В сообщении её не поискать, не
    // открыть в редакторе и не сохранить, а ссылки Telegram ещё и разворачивает
    // в предпросмотры. Автоматические отчёты по-прежнему уходят файлом только
    // когда не помещаются в сообщение.
    $sendFile = ($payload['kind'] ?? '') === 'support'
        || strlen($log) > 2500
        || (is_file($filePath) && filesize($filePath) > 4000);
    $result = signum_telegram_send($cfg, 'sendMessage', [
        'chat_id' => $cfg['chat_id'],
        'text' => $sendFile ? (substr($text, 0, 900) . "\n\nПолный отчёт — файлом.") : $text,
        'disable_web_page_preview' => 'true',
    ]);
    if (($result['ok'] ?? false) !== true) {
        return false;
    }
    if (!$sendFile) {
        return true;
    }
    if (!is_file($filePath)) {
        return false;
    }
    $doc = signum_telegram_send($cfg, 'sendDocument', [
        'chat_id' => $cfg['chat_id'],
        'document' => new CURLFile($filePath, 'application/json', basename($filePath)),
        'caption' => 'Signum ' . ($payload['kind'] ?? '') . ' ' . ($payload['platform'] ?? ''),
    ], 20);
    return ($doc['ok'] ?? false) === true;
}

/**
 * Убрать записи, которые так и не удалось отправить.
 *
 * Держим неделю: этого хватает, чтобы заметить и дослать руками, и папка не
 * растёт без конца.
 */
function signum_prune_kept_reports(string $dir, int $days = 7): void
{
    $edge = time() - $days * 86400;
    foreach (glob($dir . '/*.json') ?: [] as $file) {
        if (@filemtime($file) < $edge) {
            @unlink($file);
        }
    }
}

function signum_log_files(string $root): array
{
    // Only automatic support-session recordings are batched. Error reports
    // are not forwarded to Telegram.
    $out = glob($root . '/sessions/*.json') ?: [];
    sort($out);
    return $out;
}

function signum_short_message(string $text): string
{
    $text = trim(preg_replace('/\s+/u', ' ', $text) ?? $text);
    $text = preg_replace('/^SIGN_IPA_[A-Z0-9_]+:\s*/', '', $text) ?? $text;
    if (function_exists('mb_substr')) {
        return mb_substr($text, 0, 180);
    }
    return substr($text, 0, 180);
}

/**
 * Clients from 1.1.3 send "[CODE] english explanation". Group on the code so a
 * fault is counted once no matter how the tail of the sentence varies, and so
 * the digest reads in English.
 */
function signum_group_key(string $text): string
{
    $text = trim($text);
    if (preg_match('/^\[([A-Z][A-Z0-9_]{2,48})\]\s*(.*)$/u', $text, $m) === 1) {
        return $m[1] . ' — ' . signum_short_message($m[2]);
    }
    return signum_short_message($text);
}

function signum_telegram_digest(array $items): string
{
    $count = count($items);
    $from = $items[0]['received_at'] ?? '';
    $to = $items[$count - 1]['received_at'] ?? '';
    $platforms = [];
    $top = [];
    $sessions = 0;
    foreach ($items as $item) {
        $plat = trim(($item['platform'] ?? '?') . ' ' . ($item['version'] ?? '') . ' (' . ($item['build'] ?? '?') . ')');
        $platforms[$plat] = ($platforms[$plat] ?? 0) + 1;
        // Запись сеанса — это не ошибка, и в списке частых ошибок ей не место:
        // текста у неё нет, и она забила бы верх строкой «(без текста)».
        if (str_starts_with((string)($item['source'] ?? ''), 'auto')
            && ($item['kind'] ?? '') === 'support'
        ) {
            $sessions++;
            continue;
        }
        $key = signum_group_key((string)($item['message'] ?? ''));
        if ($key === '') {
            $key = '(без текста)';
        }
        $top[$key] = ($top[$key] ?? 0) + 1;
    }
    arsort($top);
    $hours = (int)round(SIGNUM_LOG_FLUSH_SECS / 3600);
    $errors = $count - $sessions;
    $lines = [
        'Signum · пачка логов',
        trim(
            ($errors > 0 ? $errors . ' сообщ.' : '')
            . ($errors > 0 && $sessions > 0 ? ' + ' : '')
            . ($sessions > 0 ? $sessions . ' записей сеансов' : '')
        ) . ' за ' . $hours . ' ч',
    ];
    if ($from !== '' || $to !== '') {
        $lines[] = trim($from . ' → ' . $to);
    }
    $lines[] = '';
    foreach ($platforms as $plat => $n) {
        $lines[] = $plat . ' ×' . $n;
    }
    $lines[] = '';
    if ($errors > 0) {
        $lines[] = 'Частые:';
    }
    $i = 0;
    foreach ($top as $msg => $n) {
        $lines[] = '×' . $n . '  ' . $msg;
        if (++$i >= 12) {
            break;
        }
    }
    $lines[] = '';
    $lines[] = 'Полный список — файлом. После отправки логи с сервера удалены.';
    $text = implode("\n", $lines);
    if (strlen($text) > 3500) {
        $text = substr($text, 0, 3500) . '…';
    }
    return $text;
}

function signum_telegram_batch_body(array $items): string
{
    $chunks = [];
    foreach ($items as $item) {
        $head = '[' . ($item['received_at'] ?? '') . '] '
            . ($item['platform'] ?? '?') . ' '
            . ($item['version'] ?? '') . ' (' . ($item['build'] ?? '?') . ') '
            . ($item['source'] ?? '');
        $msg = trim((string)($item['message'] ?? ''));
        $log = trim((string)($item['log'] ?? ''));
        $block = $head;
        if ($msg !== '') {
            $block .= "\n" . $msg;
        }
        if ($log !== '' && $log !== $msg) {
            $block .= "\n" . $log;
        }
        $chunks[] = $block;
    }
    $text = implode("\n\n----\n\n", $chunks);
    if (strlen($text) > 900000) {
        $text = substr($text, 0, 900000) . "\n…";
    }
    return $text;
}

/**
 * @return array{ok:bool,sent:int,reason?:string}
 */
function signum_telegram_flush(bool $onlyIfDue): array
{
    $root = signum_logs_root();
    if (!is_dir($root) && !mkdir($root, 0750, true) && !is_dir($root)) {
        return ['ok' => false, 'sent' => 0, 'reason' => 'store'];
    }
    $lockPath = $root . '/flush.lock';
    $stampPath = $root . '/last_flush';
    $lock = fopen($lockPath, 'c+');
    if ($lock === false) {
        return ['ok' => false, 'sent' => 0, 'reason' => 'lock'];
    }
    if (!flock($lock, LOCK_EX | LOCK_NB)) {
        fclose($lock);
        return ['ok' => true, 'sent' => 0, 'reason' => 'busy'];
    }

    $now = time();
    $last = is_file($stampPath) ? (int)trim((string)@file_get_contents($stampPath)) : 0;
    if ($onlyIfDue && $last > 0 && ($now - $last) < SIGNUM_LOG_FLUSH_SECS) {
        flock($lock, LOCK_UN);
        fclose($lock);
        return ['ok' => true, 'sent' => 0, 'reason' => 'wait'];
    }

    $files = signum_log_files($root);
    if ($files === []) {
        @file_put_contents($stampPath, (string)$now, LOCK_EX);
        flock($lock, LOCK_UN);
        fclose($lock);
        return ['ok' => true, 'sent' => 0, 'reason' => 'empty'];
    }

    $items = [];
    foreach ($files as $file) {
        $raw = @file_get_contents($file);
        $decoded = is_string($raw) ? json_decode($raw, true) : null;
        if (!is_array($decoded)) {
            continue;
        }
        $items[] = $decoded;
    }
    if ($items === []) {
        foreach ($files as $file) {
            @unlink($file);
        }
        @file_put_contents($stampPath, (string)$now, LOCK_EX);
        flock($lock, LOCK_UN);
        fclose($lock);
        return ['ok' => true, 'sent' => 0, 'reason' => 'empty'];
    }

    $cfg = signum_telegram_config();
    if ($cfg === null) {
        flock($lock, LOCK_UN);
        fclose($lock);
        return ['ok' => false, 'sent' => 0, 'reason' => 'tg'];
    }

    $digest = signum_telegram_digest($items);
    $result = signum_telegram_send($cfg, 'sendMessage', [
        'chat_id' => $cfg['chat_id'],
        'text' => $digest,
        'disable_web_page_preview' => 'true',
    ], 12);
    if (($result['ok'] ?? false) !== true) {
        flock($lock, LOCK_UN);
        fclose($lock);
        return ['ok' => false, 'sent' => 0, 'reason' => 'send'];
    }

    $tmp = $root . '/batch-' . date('Ymd-His') . '.txt';
    $body = signum_telegram_batch_body($items);
    $wrote = @file_put_contents($tmp, $body);
    $docOk = true;
    if ($wrote !== false && is_file($tmp)) {
        $doc = signum_telegram_send($cfg, 'sendDocument', [
            'chat_id' => $cfg['chat_id'],
            'document' => new CURLFile($tmp, 'text/plain', basename($tmp)),
            'caption' => 'Signum логи · ' . count($items) . ' шт.',
        ], 25);
        $docOk = ($doc['ok'] ?? false) === true;
        @unlink($tmp);
    }
    if (!$docOk) {
        flock($lock, LOCK_UN);
        fclose($lock);
        return ['ok' => false, 'sent' => 0, 'reason' => 'file'];
    }

    foreach ($files as $file) {
        @unlink($file);
    }
    @file_put_contents($stampPath, (string)$now, LOCK_EX);
    flock($lock, LOCK_UN);
    fclose($lock);
    return ['ok' => true, 'sent' => count($items)];
}
