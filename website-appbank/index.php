<?php
declare(strict_types=1);

$MAINT_PAGE_KEY = 'signum';

$maintGuard = __DIR__ . '/../maintenance_guard.php';
if (!is_file($maintGuard)) {
    $maintGuard = __DIR__ . '/../../maintenance_guard.php';
}

if (is_file($maintGuard)) {
    require $maintGuard;
}

if (!headers_sent()) {
    header('Content-Type: text/html; charset=UTF-8');
    header('Cache-Control: no-store, no-cache, must-revalidate, max-age=0');
    header('Pragma: no-cache');
}

readfile(__DIR__ . '/index.html');
