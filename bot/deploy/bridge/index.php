<?php
declare(strict_types=1);

$config = require dirname(__DIR__, 2) . '/signum-private/config.php';
$root = dirname(__DIR__, 2) . '/signum-private';
$key = hex2bin($config['key']);
header('Cache-Control: no-store');
header('X-Content-Type-Options: nosniff');
header('Referrer-Policy: no-referrer');
header('X-Frame-Options: DENY');
header('Strict-Transport-Security: max-age=31536000');
@ini_set('display_errors', '0');
@ini_set('zlib.output_compression', '0');
$path = parse_url($_SERVER['REQUEST_URI'] ?? '/', PHP_URL_PATH);
$method = $_SERVER['REQUEST_METHOD'] ?? 'GET';
if (empty($_SERVER['HTTPS']) || $_SERVER['HTTPS'] === 'off') {
    header('Location: ' . $config['base'] . ($_SERVER['REQUEST_URI'] ?? '/'), true, 308);
    exit;
}

function finish(int $status, array $body): never {
    http_response_code($status);
    header('Content-Type: application/json; charset=utf-8');
    echo json_encode($body, JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES);
    exit;
}

function state(callable $callback): mixed {
    global $root;
    $file = fopen($root . '/guard.json', 'c+');
    if (!$file || !flock($file, LOCK_EX)) { finish(503, ['error' => 'storage_unavailable']); }
    try {
        $raw = stream_get_contents($file);
        $data = str_starts_with($raw, 'SG1:') ? unseal(substr($raw, 4)) : (json_decode($raw, true) ?: []);
        $result = $callback($data);
        rewind($file); ftruncate($file, 0);
        fwrite($file, 'SG1:' . seal($data)); fflush($file);
        return $result;
    } finally { flock($file, LOCK_UN); fclose($file); }
}

function seal(array $value): string {
    global $key;
    $iv = random_bytes(12);
    $cipher = openssl_encrypt(json_encode($value), 'aes-256-gcm', $key, OPENSSL_RAW_DATA, $iv, $tag);
    if ($cipher === false) { finish(503, ['error' => 'encryption_unavailable']); }
    return base64_encode($iv . $cipher . $tag);
}

function unseal(string $value): array {
    global $key;
    $raw = base64_decode($value, true);
    if ($raw === false || strlen($raw) < 28) { finish(400, ['error' => 'invalid_payload']); }
    $plain = openssl_decrypt(substr($raw, 12, -16), 'aes-256-gcm', $key, OPENSSL_RAW_DATA,
        substr($raw, 0, 12), substr($raw, -16));
    $decoded = $plain === false ? null : json_decode($plain, true);
    if (!is_array($decoded)) { finish(400, ['error' => 'invalid_payload']); }
    return $decoded;
}

function metadata(string $token): ?array {
    global $root, $key;
    if (!preg_match('/^[A-Za-z0-9_-]{16,64}$/D', $token)) { return null; }
    $envelope = json_decode(@file_get_contents($root . '/files/' . $token . '.json') ?: '', true);
    if (!is_array($envelope)) { return null; }
    $raw = base64_decode($envelope['data'] ?? '', true);
    if ($raw === false || !hash_equals(hash_hmac('sha256', $raw, $key), $envelope['mac'] ?? '')) { return null; }
    $data = json_decode($raw, true);
    return is_array($data) ? $data : null;
}

function cleanup(): void {
    global $root;
    $needed = state(function (&$s) {
        if (($s['gc'] ?? 0) > time() - 60) { return false; }
        $s['gc'] = time(); return true;
    });
    if (!$needed) { return; }
    foreach (glob($root . '/queue/*') as $file) {
        $ttl = str_ends_with($file, '.evt') ? 86400 : 90;
        if (filemtime($file) < time() - $ttl) { @unlink($file); }
    }
    foreach (glob($root . '/files/*.json') as $file) {
        $token = basename($file, '.json'); $row = metadata($token);
        if ($row && (int)$row['expires_at'] <= time()) {
            @unlink($file); @unlink($root . '/files/' . $token . '.ipa');
        }
    }
    foreach (glob($root . '/files/*') as $file) {
        if (filemtime($file) < time() - 7200 && !is_file(preg_replace('/\.(ipa|part)$/', '.json', $file))) {
            @unlink($file);
        }
    }
}

function iphone_download_event(string $token, int $bytes): void {
    global $root;
    if ($bytes <= 0) { return; }
    $id = bin2hex(random_bytes(16));
    $payload = seal(['id' => $id, 'token' => $token, 'kind' => 'iphone_download_finished']);
    state(function (&$s) use ($root, $id, $payload) {
        if (count(glob($root . '/queue/*.evt')) >= 256) { return; }
        $file = $root . '/queue/' . $id;
        if (file_put_contents($file . '.part', $payload, LOCK_EX) !== false) { rename($file . '.part', $file . '.evt'); }
    });
}

if ($path === '/_bridge') {
    if ($method !== 'POST' || (int)($_SERVER['CONTENT_LENGTH'] ?? 0) > 1500000) {
        finish(400, ['error' => 'invalid_request']);
    }
    $body = file_get_contents('php://input', false, null, 0, 1500001);
    if (strlen($body) > 1500000 || !hash_equals(hash_hmac('sha256', $body, $key), $_SERVER['HTTP_X_SIGNUM_SIGNATURE'] ?? '')) {
        finish(403, ['error' => 'invalid_signature']);
    }
    $data = json_decode($body, true);
    if (!is_array($data) || abs(time() - (int)($data['time'] ?? 0)) > 60 ||
        !preg_match('/^[a-f0-9]{32}$/D', $data['nonce'] ?? '')) { finish(403, ['error' => 'expired_request']); }
    $fresh = state(function (&$s) use ($data) {
        $s['nonces'] = array_filter($s['nonces'] ?? [], fn($t) => $t > time() - 120);
        if (isset($s['nonces'][$data['nonce']]) || count($s['nonces']) >= 4096) { return false; }
        $s['nonces'][$data['nonce']] = time();
        return true;
    });
    if (!$fresh) { finish(403, ['error' => 'replayed_request']); }
    if (($data['op'] ?? '') === 'yookassa') {
        $request = unseal($data['payload'] ?? '');
        $verb = $request['method'] ?? ''; $route = $request['path'] ?? '';
        $shop = $request['shop'] ?? ''; $secret = $request['secret'] ?? '';
        $idempotence = $request['idempotence'] ?? '';
        if (($request['kind'] ?? '') !== 'yookassa' ||
            !is_string($shop) || !preg_match('/^[0-9]{1,16}$/D', $shop) ||
            !is_string($secret) || !preg_match('/^(live|test)_[A-Za-z0-9_-]{10,128}$/D', $secret) ||
            !is_string($route) || !is_string($verb) ||
            !(($verb === 'GET' && ($route === 'me' || preg_match('/^payments\/[a-fA-F0-9-]{36}$/D', $route))) ||
              ($verb === 'POST' && $route === 'payments' && is_array($request['json'] ?? null) &&
               is_string($idempotence) && preg_match('/^[A-Za-z0-9_-]{1,64}$/D', $idempotence)))) {
            finish(400, ['error' => 'invalid_payment_request']);
        }
        $content = ''; $http = curl_init('https://api.yookassa.ru/v3/' . $route);
        $headers = ['Accept: application/json', 'Content-Type: application/json'];
        if ($verb === 'POST') { $headers[] = 'Idempotence-Key: ' . $idempotence; }
        curl_setopt_array($http, [CURLOPT_CUSTOMREQUEST => $verb, CURLOPT_HTTPHEADER => $headers,
            CURLOPT_USERPWD => $shop . ':' . $secret, CURLOPT_HTTPAUTH => CURLAUTH_BASIC,
            CURLOPT_CONNECTTIMEOUT => 3, CURLOPT_TIMEOUT => 10, CURLOPT_IPRESOLVE => CURL_IPRESOLVE_V4,
            CURLOPT_SSL_VERIFYPEER => true, CURLOPT_SSL_VERIFYHOST => 2, CURLOPT_FOLLOWLOCATION => false,
            CURLOPT_WRITEFUNCTION => function ($curl, $chunk) use (&$content) {
                if (strlen($content) + strlen($chunk) > 262144) { return 0; }
                $content .= $chunk; return strlen($chunk);
            }]);
        if ($verb === 'POST') { curl_setopt($http, CURLOPT_POSTFIELDS, json_encode($request['json'])); }
        $success = curl_exec($http); $status = curl_getinfo($http, CURLINFO_HTTP_CODE); curl_close($http);
        unset($request, $secret);
        $result = json_decode($content, true);
        if ($success === false || !is_array($result)) { finish(503, ['error' => 'payment_provider_unavailable']); }
        finish(200, ['payload' => seal(['kind' => 'yookassa', 'status' => $status, 'data' => $result])]);
    }
    if (($data['op'] ?? '') === 'catalog_icon') {
        $url = $data['url'] ?? ''; $parts = is_string($url) ? parse_url($url) : false;
        if (!$parts || ($parts['scheme'] ?? '') !== 'https' ||
            !in_array($parts['host'] ?? '', ['cdn.appbank.pw', 'api.appbank.pw'], true) ||
            ($parts['port'] ?? 443) !== 443 || isset($parts['user']) || isset($parts['pass'])) {
            finish(400, ['error' => 'invalid_icon']);
        }
        $content = ''; $http = curl_init($url);
        curl_setopt_array($http, [CURLOPT_CONNECTTIMEOUT => 3, CURLOPT_TIMEOUT => 8,
            CURLOPT_IPRESOLVE => CURL_IPRESOLVE_V4, CURLOPT_SSL_VERIFYPEER => true,
            CURLOPT_SSL_VERIFYHOST => 2, CURLOPT_FOLLOWLOCATION => false,
            CURLOPT_WRITEFUNCTION => function ($curl, $chunk) use (&$content) {
                if (strlen($content) + strlen($chunk) > 2097152) { return 0; }
                $content .= $chunk; return strlen($chunk);
            }]);
        $success = curl_exec($http); $status = curl_getinfo($http, CURLINFO_HTTP_CODE); curl_close($http);
        if ($success === false || $status !== 200 || @getimagesizefromstring($content) === false) {
            finish(503, ['error' => 'icon_unavailable']);
        }
        finish(200, ['data' => base64_encode($content)]);
    }
    if (in_array($data['op'] ?? '', ['catalog', 'catalog_app'], true)) {
        $params = ['vers' => '1.1.3', 'build' => '26'];
        $endpoint = 'main.php';
        if ($data['op'] === 'catalog_app') {
            $id = $data['app_id'] ?? '';
            if (!is_string($id) || !preg_match('/^[A-Za-z0-9_-]{1,80}$/D', $id)) { finish(400, ['error' => 'invalid_app']); }
            $params['id'] = $id; $endpoint = 'info.php';
        }
        $http = curl_init('https://api.appbank.pw/signum/' . $endpoint . '?' . http_build_query($params));
        curl_setopt_array($http, [CURLOPT_RETURNTRANSFER => true, CURLOPT_CONNECTTIMEOUT => 3,
            CURLOPT_TIMEOUT => 8, CURLOPT_IPRESOLVE => CURL_IPRESOLVE_V4, CURLOPT_SSL_VERIFYPEER => true,
            CURLOPT_SSL_VERIFYHOST => 2, CURLOPT_FOLLOWLOCATION => false]);
        $response = curl_exec($http); $status = curl_getinfo($http, CURLINFO_HTTP_CODE); curl_close($http);
        if ($status !== 200 || !is_string($response) || strlen($response) > 2097152) { finish(503, ['error' => 'catalog_unavailable']); }
        $result = json_decode($response, true);
        if (!is_array($result)) { finish(503, ['error' => 'catalog_unavailable']); }
        finish(200, ['data' => $result]);
    }
    if (($data['op'] ?? '') === 'file_register') {
        $token = $data['token'] ?? ''; $envelope = $data['metadata'] ?? null;
        if (!is_string($token) || !preg_match('/^[A-Za-z0-9_-]{16,64}$/D', $token) || !is_array($envelope)) { finish(400, ['error' => 'invalid_file']); }
        $raw = base64_decode($envelope['data'] ?? '', true);
        if ($raw === false || !hash_equals(hash_hmac('sha256', $raw, $key), $envelope['mac'] ?? '')) { finish(400, ['error' => 'invalid_metadata']); }
        $row = json_decode($raw, true);
        if (!is_array($row) || empty($row['remote']) || empty($config['objects']) ||
            ($row['expires_at'] ?? 0) <= time() || ($row['expires_at'] ?? 0) > time() + 46800 ||
            ($row['size'] ?? 0) <= 0 || ($row['size'] ?? 0) > 4294967296) { finish(400, ['error' => 'invalid_metadata']); }
        $target = $root . '/files/' . $token . '.json';
        if (file_put_contents($target . '.part', json_encode($envelope), LOCK_EX) === false || !rename($target . '.part', $target)) { finish(503, ['error' => 'storage_unavailable']); }
        chmod($target, 0600); finish(200, ['stored' => true]);
    }
    if (in_array($data['op'] ?? '', ['file_begin', 'file_chunk', 'file_finish', 'file_delete', 'file_status'], true)) {
        $token = $data['token'] ?? '';
        if (!is_string($token) || !preg_match('/^[A-Za-z0-9_-]{16,64}$/D', $token)) { finish(400, ['error' => 'invalid_token']); }
        $file = $root . '/files/' . $token;
        if ($data['op'] === 'file_delete') {
            foreach (['.json', '.ipa', '.part', '.partmeta', '.json.part'] as $suffix) {
                if (is_file($file . $suffix) && !unlink($file . $suffix)) { finish(503, ['error' => 'storage_unavailable']); }
            }
            finish(200, ['ok' => true]);
        }
        if ($data['op'] === 'file_status') { finish(200, ['exists' => is_file($file . '.ipa')]); }
        if ($data['op'] === 'file_begin') {
            $size = $data['size'] ?? null;
            if (!is_int($size) || $size <= 0 || $size > 4294967296) { finish(400, ['error' => 'invalid_size']); }
            if (is_file($file . '.ipa') || is_file($file . '.json')) { finish(409, ['error' => 'file_exists']); }
            $free = disk_free_space($root . '/files');
            if ($free === false || $free < $size + 67108864) { finish(507, ['error' => 'storage_full']); }
            if (file_put_contents($file . '.part', '', LOCK_EX) === false ||
                file_put_contents($file . '.partmeta', (string)$size, LOCK_EX) === false) { finish(503, ['error' => 'storage_unavailable']); }
            finish(200, ['ok' => true]);
        }
        if (!is_file($file . '.partmeta') || !is_file($file . '.part')) { finish(404, ['error' => 'upload_missing']); }
        $expected = (int)file_get_contents($file . '.partmeta');
        $stream = fopen($file . '.part', 'c+b');
        if (!$stream || !flock($stream, LOCK_EX)) { finish(503, ['error' => 'storage_unavailable']); }
        $size = fstat($stream)['size'];
        if ($data['op'] === 'file_chunk') {
            $chunk = is_string($data['data'] ?? null) ? base64_decode($data['data'], true) : false;
            if ($chunk === false || strlen($chunk) === 0 || strlen($chunk) > 524288 ||
                !is_int($data['offset'] ?? null) || $data['offset'] !== $size || $size + strlen($chunk) > $expected) {
                flock($stream, LOCK_UN); fclose($stream); finish(409, ['error' => 'invalid_chunk']);
            }
            fseek($stream, 0, SEEK_END);
            $written = fwrite($stream, $chunk); fflush($stream);
            flock($stream, LOCK_UN); fclose($stream);
            if ($written !== strlen($chunk)) { finish(503, ['error' => 'storage_unavailable']); }
            finish(200, ['size' => $size + $written]);
        }
        $envelope = $data['metadata'] ?? null;
        $raw = is_array($envelope) && is_string($envelope['data'] ?? null) ? base64_decode($envelope['data'], true) : false;
        $row = $raw === false ? null : json_decode($raw, true);
        $valid = $raw !== false && is_string($envelope['mac'] ?? null) && hash_equals(hash_hmac('sha256', $raw, $key), $envelope['mac']);
        if (!$valid || !is_array($row) || ($row['size'] ?? null) !== $expected || $size !== $expected ||
            !is_string($data['sha256'] ?? null) || !preg_match('/^[a-f0-9]{64}$/D', $data['sha256']) ||
            !hash_equals(hash_file('sha256', $file . '.part'), $data['sha256'])) {
            flock($stream, LOCK_UN); fclose($stream); finish(409, ['error' => 'invalid_file']);
        }
        $saved = file_put_contents($file . '.json.part', json_encode($envelope), LOCK_EX) !== false &&
            rename($file . '.part', $file . '.ipa') && rename($file . '.json.part', $file . '.json');
        flock($stream, LOCK_UN); fclose($stream);
        if (!$saved) { finish(503, ['error' => 'storage_unavailable']); }
        @unlink($file . '.partmeta');
        finish(200, ['ok' => true]);
    }
    if (($data['op'] ?? '') === 'pull') {
        state(function (&$s) { $s['heartbeat'] = time(); });
        cleanup();
        $items = []; $capacity = max(0, min(16, (int)($data['capacity'] ?? 0)));
        foreach (glob($root . '/queue/*.req') as $file) {
            if (count($items) >= $capacity) { break; }
            if (!@rename($file, substr($file, 0, -4) . '.work')) { continue; }
            $items[] = ['id' => basename($file, '.req'), 'payload' => file_get_contents(substr($file, 0, -4) . '.work')];
        }
        $events = [];
        foreach (glob($root . '/queue/*.evt') as $file) {
            if (count($items) + count($events) >= $capacity) { break; }
            $events[] = ['id' => basename($file, '.evt'), 'payload' => file_get_contents($file)];
        }
        finish(200, ['requests' => $items, 'events' => $events]);
    }
    if (($data['op'] ?? '') === 'event_ack' && preg_match('/^[a-f0-9]{32}$/D', $data['id'] ?? '')) {
        @unlink($root . '/queue/' . $data['id'] . '.evt');
        finish(200, ['ok' => true]);
    }
    if (($data['op'] ?? '') === 'reply' && preg_match('/^[a-f0-9]{32}$/D', $data['id'] ?? '')) {
        $file = $root . '/queue/' . $data['id'];
        unseal($data['payload'] ?? '');
        if (is_file($file . '.work') && filemtime($file . '.work') >= time() - 26) {
            file_put_contents($file . '.res.part', $data['payload'], LOCK_EX);
            rename($file . '.res.part', $file . '.res'); @unlink($file . '.work');
        }
        finish(200, ['ok' => true]);
    }
    finish(400, ['error' => 'invalid_operation']);
}

$ip = $_SERVER['REMOTE_ADDR'] ?? '';
if (!filter_var($ip, FILTER_VALIDATE_IP)) { finish(400, ['error' => 'invalid_peer']); }
$allowed = state(function (&$s) use ($ip) {
    $s['ips'] = array_filter($s['ips'] ?? [], fn($r) => $r['time'] > time() - 60);
    $global = $s['rate'] ?? ['time' => time(), 'count' => 0];
    if ($global['time'] <= time() - 60) { $global = ['time' => time(), 'count' => 0]; }
    $row = $s['ips'][$ip] ?? ['time' => time(), 'count' => 0];
    if ($row['count'] >= 120 || $global['count'] >= 1200 || count($s['ips']) >= 4096) { return false; }
    $row['count']++; $global['count']++; $s['ips'][$ip] = $row; $s['rate'] = $global; return true;
});
if (!$allowed) { header('Retry-After: 60'); finish(429, ['error' => 'rate_limit_exceeded']); }

if ($path === '/healthz') {
    $alive = state(fn(&$s) => ($s['heartbeat'] ?? 0) > time() - 30);
    finish($alive ? 200 : 503, ['ok' => $alive, 'files' => true]);
}
if (in_array($path, ['/', '/api/docs', '/api/panel', '/api/schema.json'], true) && in_array($method, ['GET', 'HEAD'], true)) {
    $asset = match ($path) { '/api/panel' => 'panel', '/api/schema.json' => 'schema', default => 'docs' };
    $record = json_decode(@file_get_contents(__DIR__ . '/' . $asset . '.json') ?: '', true);
    if (!$record) { finish(503, ['error' => 'page_unavailable']); }
    foreach ($record['headers'] as $name => $value) { header($name . ': ' . $value); }
    if ($method === 'GET') { echo $record['body']; } exit;
}
if (preg_match('#^/(f|ota|i)/([A-Za-z0-9_-]{16,64})(?:\.(ipa|plist))?$#D', $path, $m)) {
    if (!in_array($method, ['GET', 'HEAD'], true)) { finish(405, ['error' => 'method_not_allowed']); }
    $token = $m[2]; $row = metadata($token); $file = $root . '/files/' . $token . '.ipa';
    $remote = !empty($row['remote']) && !empty($config['objects']);
    if (!$row || (int)$row['expires_at'] <= time() || (!$remote && !is_file($file))) { finish(404, ['error' => 'link_expired']); }
    $iphone = preg_match('/iPhone|iOS[\/ ]|itunesstored|appstored|MobileInstallation/i', $_SERVER['HTTP_USER_AGENT'] ?? '') === 1;
    if ($method === 'GET' && in_array($m[1], ['ota', 'i'], true) && $iphone) {
        state(function (&$s) use ($token) {
            $s['iphone_tokens'] = array_filter($s['iphone_tokens'] ?? [], fn($t) => $t > time() - 600);
            if (count($s['iphone_tokens']) < 4096) { $s['iphone_tokens'][$token] = time(); }
        });
    }
    if ($m[1] !== 'f') {
        header('Content-Type: ' . ($m[1] === 'i' ? 'text/html; charset=utf-8' : 'application/xml'));
        if ($m[1] === 'i') { header("Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; img-src https:; base-uri 'none'; frame-ancestors 'none'"); }
        if ($method === 'GET') { echo $m[1] === 'i' ? $row['page'] : base64_decode($row['manifest']); } exit;
    }
    $size = $remote ? (int)$row['size'] : filesize($file); $start = 0; $end = $size - 1;
    if (isset($_SERVER['HTTP_RANGE'])) {
        if (!preg_match('/^bytes=(\d*)-(\d*)$/D', $_SERVER['HTTP_RANGE'], $range) || ($range[1] === '' && $range[2] === '')) {
            header('Content-Range: bytes */' . $size); finish(416, ['error' => 'invalid_range']);
        }
        if ($range[1] === '') { $start = max(0, $size - (int)$range[2]); }
        else { $start = (int)$range[1]; if ($range[2] !== '') { $end = min($end, (int)$range[2]); } }
        if ($start > $end || $start >= $size) { header('Content-Range: bytes */' . $size); finish(416, ['error' => 'invalid_range']); }
        http_response_code(206); header("Content-Range: bytes $start-$end/$size");
    }
    $lease = bin2hex(random_bytes(8));
    $admitted = state(function (&$s) use ($lease, $ip) {
        $s['leases'] = array_filter($s['leases'] ?? [], fn($r) => $r['time'] > time() - 45);
        $mine = count(array_filter($s['leases'], fn($r) => $r['ip'] === $ip));
        if (count($s['leases']) >= 12 || $mine >= 3) { return false; }
        $s['leases'][$lease] = ['time' => time(), 'ip' => $ip]; return true;
    });
    if (!$admitted) { header('Retry-After: 30'); finish(429, ['error' => 'too_many_downloads']); }
    register_shutdown_function(function () use ($lease) { state(function (&$s) use ($lease) { unset($s['leases'][$lease]); }); });
    if ($remote) {
        $expires = time() + 240;
        $signature = hash_hmac('sha256', $token . ':' . $expires, hex2bin($config['objects']['key']));
        $origin = rtrim($config['objects']['url'], '/') . '/f/' . $token . '.ipa?expires=' . $expires . '&signature=' . $signature;
        $curl = curl_init($origin);
        curl_setopt_array($curl, [CURLOPT_NOBODY => true, CURLOPT_RETURNTRANSFER => true,
            CURLOPT_CONNECTTIMEOUT => 5, CURLOPT_TIMEOUT => 10, CURLOPT_FOLLOWLOCATION => false,
            CURLOPT_SSL_VERIFYPEER => true, CURLOPT_SSL_VERIFYHOST => 2,
            CURLOPT_RANGE => $start . '-' . $end]);
        $ready = curl_exec($curl) !== false && in_array(curl_getinfo($curl, CURLINFO_RESPONSE_CODE), [200, 206], true);
        if (!$ready) { curl_close($curl); finish(503, ['error' => 'file_unavailable']); }
    }
    header('Content-Type: application/octet-stream'); header('Accept-Ranges: bytes');
    header('Content-Length: ' . ($end - $start + 1));
    header('Content-Disposition: attachment; filename="Signum.ipa"; filename*=UTF-8\'\'' . rawurlencode($row['name']));
    if ($method === 'HEAD') { exit; }
    $iphone = $iphone || state(fn(&$s) => ($s['iphone_tokens'][$token] ?? 0) > time() - 600);
    $sent = 0;
    if ($iphone) { register_shutdown_function(function () use ($token, &$sent) { iphone_download_event($token, $sent); }); }
    @set_time_limit(0); while (ob_get_level()) { ob_end_clean(); }
    if ($remote) {
        $synced = time();
        curl_setopt_array($curl, [CURLOPT_NOBODY => false, CURLOPT_RETURNTRANSFER => false, CURLOPT_TIMEOUT => 0,
            CURLOPT_LOW_SPEED_LIMIT => 1024, CURLOPT_LOW_SPEED_TIME => 60,
            CURLOPT_WRITEFUNCTION => function ($handle, $chunk) use (&$sent, &$synced, $lease, $ip) {
                if (connection_aborted()) { return 0; }
                $sent += strlen($chunk); echo $chunk; flush();
                if (time() - $synced >= 2) {
                    state(function (&$s) use ($lease, $ip) { $s['leases'][$lease] = ['time' => time(), 'ip' => $ip]; }); $synced = time();
                }
                return strlen($chunk);
            }]);
        curl_exec($curl); curl_close($curl); exit;
    }
    $stream = fopen($file, 'rb'); fseek($stream, $start); $left = $end - $start + 1; $synced = time();
    while ($left > 0 && !feof($stream) && !connection_aborted()) {
        $chunk = fread($stream, min(262144, $left)); if ($chunk === false || $chunk === '') { break; }
        $sent += strlen($chunk); echo $chunk; flush(); $left -= strlen($chunk);
        if (time() - $synced >= 2) {
            state(function (&$s) use ($lease, $ip) { $s['leases'][$lease] = ['time' => time(), 'ip' => $ip]; }); $synced = time();
        }
    }
    fclose($stream); exit;
}

if (!(str_starts_with($path, '/api/v1/') || $path === '/payments/yookassa') ||
    !in_array($method, ['GET', 'POST', 'PATCH', 'DELETE'], true)) { finish(404, ['error' => 'not_found']); }
if (strlen($_SERVER['REQUEST_URI']) > 4096 || (int)($_SERVER['CONTENT_LENGTH'] ?? 0) > 16384) { finish(413, ['error' => 'request_too_large']); }
$body = file_get_contents('php://input', false, null, 0, 16385);
if (strlen($body) > 16384) { finish(413, ['error' => 'request_too_large']); }
$headers = []; $names = ['HTTP_AUTHORIZATION' => 'Authorization', 'CONTENT_TYPE' => 'Content-Type', 'HTTP_IDEMPOTENCY_KEY' => 'Idempotency-Key'];
foreach ($names as $source => $name) { if (isset($_SERVER[$source])) { $headers[$name] = $_SERVER[$source]; } }
if (!isset($headers['Authorization'])) {
    if (isset($_SERVER['REDIRECT_HTTP_AUTHORIZATION'])) { $headers['Authorization'] = $_SERVER['REDIRECT_HTTP_AUTHORIZATION']; }
    elseif (function_exists('getallheaders')) {
        foreach (getallheaders() as $name => $value) {
            if (strcasecmp($name, 'Authorization') === 0) { $headers['Authorization'] = $value; break; }
        }
    }
}
$id = bin2hex(random_bytes(16)); $file = $root . '/queue/' . $id;
$payload = seal(['path' => $_SERVER['REQUEST_URI'], 'method' => $method, 'headers' => (object)$headers,
    'ip' => $ip, 'body' => base64_encode($body), 'deadline' => microtime(true) + 24]);
$admitted = state(function (&$s) use ($root, $file, $payload) {
    if (($s['heartbeat'] ?? 0) <= time() - 30 || count(glob($root . '/queue/*.req')) + count(glob($root . '/queue/*.work')) >= 32) { return false; }
    if (file_put_contents($file . '.part', $payload, LOCK_EX) === false || !rename($file . '.part', $file . '.req')) { return false; }
    return true;
});
if (!$admitted) { finish(503, ['error' => 'server_busy']); }
$deadline = microtime(true) + 25;
while (microtime(true) < $deadline && !connection_aborted()) {
    clearstatcache(true, $file . '.res');
    if (is_file($file . '.res')) {
        $result = unseal(file_get_contents($file . '.res')); @unlink($file . '.res');
        http_response_code((int)$result['status']); header('Content-Type: ' . $result['type']);
        echo base64_decode($result['body']); exit;
    }
    usleep(100000);
}
@unlink($file . '.req'); @unlink($file . '.work'); @unlink($file . '.res');
finish(504, ['error' => 'bridge_timeout']);
