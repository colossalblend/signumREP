<?php
declare(strict_types=1);

function signum_site_releases(?string $manifestPath = null): array
{
    $manifestPath = $manifestPath ?? dirname(__DIR__, 2) . '/api.appbank.pw/signum/releases.php';
    if (!is_file($manifestPath)) {
        throw new RuntimeException('Release manifest unavailable');
    }
    $releases = require $manifestPath;
    if (!is_array($releases)) {
        throw new RuntimeException('Invalid release manifest');
    }
    $result = [];
    foreach (['windows', 'macos', 'linux', 'android'] as $platform) {
        $release = $releases[$platform] ?? null;
        if (!is_array($release)) {
            continue;
        }
        $path = (string)($release['download_url'] ?? '');
        $sha = (string)($release['sha256'] ?? '');
        if (!preg_match('~^/releases/[A-Za-z0-9_.-]+$~D', $path)
            || strpos($path, '..') !== false
            || !preg_match('/^[a-f0-9]{64}$/iD', $sha)
            || empty($release['version'])) {
            throw new RuntimeException('Invalid release metadata');
        }
        $file = dirname($manifestPath) . $path;
        if (!is_file($file) || filesize($file) === 0) {
            throw new RuntimeException('Release file unavailable');
        }
        $result[$platform] = [
            'version' => (string)$release['version'],
            'build' => (string)($release['build'] ?? ''),
            'published_at' => $release['published_at'] ?? null,
            'download_url' => 'https://api.appbank.pw/signum' . $path,
            'sha256' => strtolower($sha),
            'size' => filesize($file),
        ];
        if ($platform === 'linux') {
            $debPath = preg_replace('~/([^/]+)\.AppImage$~', '/Signum_$1_amd64.deb', $path);
            if ($debPath !== $path && is_file(dirname($manifestPath) . $debPath)) {
                $result[$platform]['deb_download_url'] = 'https://api.appbank.pw/signum' . $debPath;
                $result[$platform]['deb_available'] = true;
            } else {
                $result[$platform]['deb_available'] = false;
            }
        }
    }
    return $result;
}
