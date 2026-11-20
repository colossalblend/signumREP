<?php
declare(strict_types=1);

   
                                                   
  
            
                                 
                                                 
  
                     
                                                                                    
                                                                                    
                                                  
   

ini_set('display_errors', '1');
ini_set('display_startup_errors', '1');
error_reporting(E_ALL);

require_once __DIR__ . '/common.php';

signum_handle_options();

$client = signum_client_context();
signum_require_min_client($client);

$type = strtolower(trim((string)($_GET['type'] ?? $_REQUEST['type'] ?? '')));

if ($type === '' || $type === 'help') {
    signum_json_response([
        'ok' => true,
        'api_version' => SIGNUM_API_VERSION,
        'min_client_version' => signum_ota_min_client_version((string)($client['platform'] ?? 'windows')),
        'types' => [
            'auto_purchase' => 'Список App Store-приложений банков/сервисов для автопокупки на Apple ID',
        ],
        'client' => $client,
    ]);
}

if ($type === 'auto_purchase') {
    $payload = signum_auto_purchase_payload($client['vers'] !== '' ? $client['vers'] : null);
    $known = trim((string)($_GET['known'] ?? $_REQUEST['known'] ?? ''));
    $hasNew = false;
    if ($known !== '' && $payload['enabled'] && $payload['fingerprint'] !== $known) {
        $hasNew = true;
    }

    signum_json_response([
        'ok' => true,
        'api_version' => SIGNUM_API_VERSION,
        'min_client_version' => signum_ota_min_client_version((string)($client['platform'] ?? 'windows')),
        'server_time' => gmdate('c'),
        'client' => $client,
        'has_new' => $hasNew,
        'known' => $known !== '' ? $known : null,
    ] + $payload);
}

signum_json_response([
    'ok' => false,
    'error' => 'Неизвестный type. Используйте type=auto_purchase',
    'type' => $type,
], 400);
