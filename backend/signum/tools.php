<?php
declare(strict_types=1);

   
                                                       
  
                                                                             
                                                                             
                                                      
  
           
                                 
   

require_once __DIR__ . '/common.php';

signum_handle_options();

const SIGNUM_TOOLS_MANIFEST_SCHEMA = 1;
const SIGNUM_TOOLS_MAX_ARTIFACT_BYTES = 134217728;           

function signum_tools_error(string $message, int $status = 400): void
{
    signum_json_response([
        'ok' => false,
        'error' => 'tools_unavailable',
        'message' => $message,
    ], $status);
}

function signum_tools_safe_identifier(string $value): bool
{
    return (bool)preg_match('/^[a-z0-9][a-z0-9._-]{0,95}$/', $value);
}

function signum_tools_safe_file_name(string $value): bool
{
    return $value !== ''
        && basename($value) === $value
        && !str_contains($value, '..')
        && (bool)preg_match('/^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/', $value);
}

function signum_tools_normalize_sha256(string $value): ?string
{
    $value = strtolower(trim($value));
    return preg_match('/^[a-f0-9]{64}$/', $value) ? $value : null;
}

function signum_tools_base_dir(): string
{
    return __DIR__ . '/files/tools';
}

function signum_tools_load_manifest(): array
{
    $path = signum_tools_base_dir() . '/manifest.json';
    if (!is_file($path)) {
        signum_tools_error('Обновления компонентов пока не опубликованы.', 503);
    }
    $raw = file_get_contents($path);
                                                                            
    $raw = is_string($raw) ? preg_replace('/^\\xEF\\xBB\\xBF/', '', $raw) : false;
    $manifest = is_string($raw) ? json_decode($raw, true) : null;
    if (!is_array($manifest) || (int)($manifest['schema_version'] ?? 0) !== SIGNUM_TOOLS_MANIFEST_SCHEMA) {
        signum_tools_error('Конфигурация обновлений на сервере недействительна.', 503);
    }
    return $manifest;
}

function signum_tools_asset_payload(string $target, string $name, array $asset): ?array
{
    if (!signum_tools_safe_identifier($target) || !signum_tools_safe_identifier($name)) {
        return null;
    }

    $version = trim((string)($asset['version'] ?? ''));
    $file = trim((string)($asset['file'] ?? ''));
    $expectedSize = (int)($asset['size'] ?? 0);
    $expectedSha256 = signum_tools_normalize_sha256((string)($asset['sha256'] ?? ''));
    if ($version === '' || !signum_tools_safe_file_name($file) || $expectedSize < 1
        || $expectedSize > SIGNUM_TOOLS_MAX_ARTIFACT_BYTES || $expectedSha256 === null) {
        return null;
    }

    $path = signum_tools_base_dir() . '/' . $target . '/' . $file;
    if (!is_file($path) || filesize($path) !== $expectedSize) {
        return null;
    }
    $actualSha256 = hash_file('sha256', $path);
    if (!is_string($actualSha256) || !hash_equals($expectedSha256, strtolower($actualSha256))) {
        return null;
    }

    return [
        'version' => $version,
        'path' => 'files/tools/' . rawurlencode($target) . '/' . rawurlencode($file),
        'sha256' => $expectedSha256,
        'size' => $expectedSize,
    ];
}

$target = strtolower(trim((string)($_GET['target'] ?? $_REQUEST['target'] ?? '')));
$allowedTargets = ['windows-x64', 'macos-x64', 'macos-arm64', 'linux-x64', 'linux-arm64'];
if (!in_array($target, $allowedTargets, true)) {
    signum_tools_error('Неподдерживаемая платформа.');
}

$manifest = signum_tools_load_manifest();
$targetManifest = $manifest['targets'][$target] ?? null;
if (!is_array($targetManifest)) {
    signum_json_response([
        'ok' => true,
        'schema_version' => SIGNUM_TOOLS_MANIFEST_SCHEMA,
        'target' => $target,
        'tools' => new stdClass(),
        'server_time' => gmdate('c'),
    ]);
}

$tools = [];
foreach ($targetManifest as $name => $asset) {
    if (!is_string($name) || !is_array($asset)) {
        continue;
    }
    $payload = signum_tools_asset_payload($target, $name, $asset);
    if ($payload !== null) {
        $tools[$name] = $payload;
    }
}
ksort($tools, SORT_STRING);

signum_json_response([
    'ok' => true,
    'schema_version' => SIGNUM_TOOLS_MANIFEST_SCHEMA,
    'target' => $target,
    'tools' => $tools,
    'server_time' => gmdate('c'),
]);
