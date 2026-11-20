<?php
declare(strict_types=1);

   
                                                          
  
                           
                           
  
                              
  
            
    
                
                
                           
                                    
                                                
                        
                      
    
   

ini_set('serialize_precision', '-1');
ini_set('display_errors', '0');
error_reporting(E_ALL);

const LINK_CACHE_SECONDS = 600;                                  
const BAN_SECONDS = 180;                                 
const MAX_REQUESTS_PER_MIN = 15;

const DEFAULT_PUBLIC_KEY_BODY =
'MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAr3yeO0wqBf70fZczGxZe' .
'vUhJdX26QZyuOvVFz1vwvsk81mnGDRIrXwgUBUpeFiDeH/CEJDkRciESGb2fbgrz' .
'cZhe5Rkr2jrmUKJLP5L00fhFNzGxUAB7KKcWawJt+OgO21T7/Jb3rEOkR/MFzA6/' .
'ZgXkfXqpEEMEsyt4JrwKbueSZDevbcPnzX5VUQNh+p6ETTx86XwQrkC2nT9okilI' .
'Qm+k3fhWacVy1CMoQrjZp/XwQZEbirHad+cANDjv5xROymlEKP11RijQMCQgOoIE' .
'Tr3aS/AiRaLGE1Ik04XvQwln2MUaOP7qFhiqkMsdWnKiPpD6m+kuKdsMLymtR+jb' .
'GwIDAQAB';

function tb_json_response(array $payload, int $status = 200): void
{
    http_response_code($status);
    header('Content-Type: application/json; charset=utf-8');
    header('Access-Control-Allow-Origin: *');
    header('Access-Control-Allow-Methods: GET, OPTIONS');
    header('Access-Control-Allow-Headers: Content-Type');
    header('Cache-Control: no-store, no-cache, must-revalidate, max-age=0');
    header('Pragma: no-cache');
    echo json_encode($payload, JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES);
    exit;
}

if (($_SERVER['REQUEST_METHOD'] ?? 'GET') === 'OPTIONS') {
    tb_json_response(['ok' => true]);
}

function publicKeyPem(): string
{
    return "-----BEGIN PUBLIC KEY-----\n"
        . chunk_split(DEFAULT_PUBLIC_KEY_BODY, 64, "\n")
        . "-----END PUBLIC KEY-----\n";
}

function normalizeScheme(string $scheme): string
{
    $scheme = trim($scheme);
    $scheme = preg_replace('~://.*$~', '', $scheme) ?? $scheme;
    $scheme = preg_replace('~[^a-zA-Z0-9+.-]~', '', $scheme) ?? $scheme;
    return strtolower($scheme);
}

function encryptedPayloadToken(string $userId, float $timestamp): string
{
    $payload = json_encode(
        ['u' => $userId, 't' => $timestamp],
        JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR
    );

    $publicKey = openssl_pkey_get_public(publicKeyPem());
    if ($publicKey === false) {
        throw new RuntimeException('OpenSSL key parse failed');
    }

    $ok = openssl_public_encrypt($payload, $encrypted, $publicKey, OPENSSL_PKCS1_PADDING);
    if (!$ok) {
        throw new RuntimeException('OpenSSL encryption failed');
    }

    return rtrim(strtr(base64_encode($encrypted), '+/', '-_'), '=');
}

function buildActivationLink(string $scheme, string $token): string
{
    return normalizeScheme($scheme) . '://' . $token;
}

function getUserIp(): string
{
    if (!empty($_SERVER['HTTP_CF_CONNECTING_IP'])) {
        return (string)$_SERVER['HTTP_CF_CONNECTING_IP'];
    }
    if (!empty($_SERVER['HTTP_X_FORWARDED_FOR'])) {
        $ips = explode(',', (string)$_SERVER['HTTP_X_FORWARDED_FOR']);
        return trim($ips[0]);
    }
    return (string)($_SERVER['REMOTE_ADDR'] ?? '127.0.0.1');
}

                                          
$appId = trim((string)($_GET['id'] ?? $_REQUEST['id'] ?? $_REQUEST['appId'] ?? ''));

if ($appId === '') {
    tb_json_response([
        'ok' => false,
        'error' => 'Не передан параметр id (например, ?id=sn или ?id=vm)',
    ], 400);
}

$appsData = [];
$jsonFilePath = __DIR__ . '/tb.json';
if (file_exists($jsonFilePath)) {
    $parsedJson = json_decode((string)file_get_contents($jsonFilePath), true);
    if (is_array($parsedJson)) {
        foreach ($parsedJson as $key => $item) {
            if (!is_array($item)) {
                continue;
            }
            $id = (string)($item['id'] ?? $key);
            $appsData[$id] = $item;
        }
    }
}

if (!isset($appsData[$appId])) {
    tb_json_response([
        'ok' => false,
        'error' => 'Приложение с таким id не найдено в tb.json',
        'id' => $appId,
    ], 404);
}

$entry = $appsData[$appId];
$scheme = (string)($entry['deeplink'] ?? '');
if ($scheme === '') {
    tb_json_response([
        'ok' => false,
        'error' => 'У приложения не настроен deeplink',
        'id' => $appId,
    ], 400);
}

$now = time();
$ip = getUserIp();
$ipHash = md5($ip . '_' . $appId);

$cacheDir = __DIR__ . '/cache';
if (!is_dir($cacheDir)) {
    @mkdir($cacheDir, 0777, true);
}

$dataFile = $cacheDir . '/data_' . $ipHash . '.json';
$userData = [
    'ban_until' => 0,
    'hits' => [],
    'cached_link' => '',
    'cached_time' => 0,
];

if (file_exists($dataFile)) {
    $fileContent = file_get_contents($dataFile);
    if ($fileContent) {
        $userData = array_merge($userData, json_decode($fileContent, true) ?? []);
    }
}

if ((int)$userData['ban_until'] > $now) {
    $timeLeft = (int)$userData['ban_until'] - $now;
    tb_json_response([
        'ok' => false,
        'error' => "Слишком много запросов. Подождите {$timeLeft} сек.",
        'retry_after' => $timeLeft,
    ], 429);
}

$userData['hits'] = array_values(array_filter(
    $userData['hits'] ?? [],
    static fn($timestamp): bool => (int)$timestamp > ($now - 60)
));
$userData['hits'][] = $now;

if (count($userData['hits']) > MAX_REQUESTS_PER_MIN) {
    $userData['ban_until'] = $now + BAN_SECONDS;
    $userData['hits'] = [];
    file_put_contents($dataFile, json_encode($userData), LOCK_EX);
    tb_json_response([
        'ok' => false,
        'error' => 'Обнаружена подозрительная активность. Доступ ограничен на 3 минуты.',
        'retry_after' => BAN_SECONDS,
    ], 429);
}

$activationLink = '';
$fromCache = false;
if (
    (int)($userData['cached_time'] ?? 0) > ($now - LINK_CACHE_SECONDS)
    && !empty($userData['cached_link'])
) {
    $activationLink = (string)$userData['cached_link'];
    $fromCache = true;
} else {
    try {
        $userId = '1';
                                                                        
        $timestamp = microtime(true) - 60;
        $token = encryptedPayloadToken($userId, $timestamp);
        $activationLink = buildActivationLink($scheme, $token);
        $userData['cached_link'] = $activationLink;
        $userData['cached_time'] = $now;
    } catch (Throwable $e) {
        tb_json_response([
            'ok' => false,
            'error' => 'Внутренняя ошибка шифрования',
        ], 500);
    }
}

file_put_contents($dataFile, json_encode($userData), LOCK_EX);

$cachedAt = (int)($userData['cached_time'] ?? $now);
$expiresIn = max(0, LINK_CACHE_SECONDS - ($now - $cachedAt));

tb_json_response([
    'ok' => true,
    'id' => $appId,
    'name' => (string)($entry['name'] ?? $appId),
    'scheme' => normalizeScheme($scheme),
    'qr_payload' => $activationLink,
    'mode' => 'deeplink',
    'app_store_id' => (string)($entry['app_store_id'] ?? ''),
    'bundle_id' => (string)($entry['bundle_id'] ?? ''),
    'from_cache' => $fromCache,
    'expires_in' => $expiresIn,
    'hint' => 'Отсканируйте QR камерой iPhone — откроется приложение для активации.',
]);
