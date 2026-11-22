<?php
declare(strict_types=1);

/**
 * Resolves the release manifest and returns the validated data for one
 * platform: the static file path (relative to the API host) plus metadata.
 *
 * @return array{path:string,file:string,release:array}|null
 */
function signum_site_release_entry(string $platform, bool $wantDeb = false, ?string $manifestPath = null): ?array
{
    $manifestPath = $manifestPath ?? dirname(__DIR__, 2) . '/api.appbank.pw/signum/releases.php';
    if (!is_file($manifestPath)) {
        throw new RuntimeException('Release manifest unavailable');
    }
    $releases = require $manifestPath;
    if (!is_array($releases)) {
        throw new RuntimeException('Invalid release manifest');
    }
    $release = $releases[$platform] ?? null;
    if (!is_array($release)) {
        return null;
    }

    $path = (string)($release['download_url'] ?? '');
    $sha = (string)($release['sha256'] ?? '');
    if (!preg_match('~^/releases/[A-Za-z0-9_.-]+$~D', $path)
        || strpos($path, '..') !== false
        || !preg_match('/^[a-f0-9]{64}$/iD', $sha)
        || empty($release['version'])) {
        throw new RuntimeException('Invalid release metadata');
    }

    if ($wantDeb) {
        if ($platform !== 'linux') {
            return null;
        }
        if (preg_match('~\.deb$~D', $path)) {
            // The deb is the primary Linux artefact for this release.
            $debPath = $path;
        } else {
            $debPath = (string)preg_replace('~/([^/]+)\.AppImage$~', '/Signum_$1_amd64.deb', $path);
            if ($debPath === $path
                || !preg_match('~^/releases/[A-Za-z0-9_.-]+$~D', $debPath)
                || strpos($debPath, '..') !== false) {
                return null;
            }
        }
        $debFile = dirname($manifestPath) . $debPath;
        if (!is_file($debFile) || filesize($debFile) === 0) {
            return null;
        }
        $release['deb_download_url'] = $debPath;
        $release['deb_sha256'] = (string)($release['deb_sha256'] ?? $sha);
        return ['path' => $debPath, 'file' => $debFile, 'release' => $release];
    }

    $file = dirname($manifestPath) . $path;
    if (!is_file($file) || filesize($file) === 0) {
        throw new RuntimeException('Release file unavailable');
    }
    return ['path' => $path, 'file' => $file, 'release' => $release];
}

/**
 * Returns the static file path (e.g. "/releases/1.2.3-b43.exe") for the
 * requested platform, or null when that artefact is unavailable.
 */
function signum_site_release_path(string $platform, bool $wantDeb = false, ?string $manifestPath = null): ?string
{
    $entry = signum_site_release_entry($platform, $wantDeb, $manifestPath);
    return $entry === null ? null : $entry['path'];
}

/**
 * Public release data for the site UI. Download links stay on this host
 * (download.php), so the api.appbank.pw host never leaks into page HTML;
 * the direct static URL is only used inside the redirect/fallback.
 */
function signum_site_releases(?string $manifestPath = null): array
{
    $manifestPath = $manifestPath ?? dirname(__DIR__, 2) . '/api.appbank.pw/signum/releases.php';
    $base = signum_site_base_url();
    $result = [];
    foreach (['windows', 'macos', 'linux', 'android'] as $platform) {
        $entry = signum_site_release_entry($platform, false, $manifestPath);
        if ($entry === null) {
            continue;
        }
        $release = $entry['release'];
        $result[$platform] = [
            'version' => (string)$release['version'],
            'build' => (string)($release['build'] ?? ''),
            'published_at' => $release['published_at'] ?? null,
            'download_url' => $base . '/download.php?platform=' . $platform,
            'sha256' => strtolower((string)$release['sha256']),
            'size' => filesize($entry['file']),
        ];
        if ($platform === 'linux') {
            $result[$platform]['deb_available'] = signum_site_release_path('linux', true, $manifestPath) !== null;
            if ($result[$platform]['deb_available']) {
                $result[$platform]['deb_download_url'] = $base . '/download.php?platform=linux&format=deb';
            }
        }
    }
    return $result;
}

/**
 * Absolute base URL of the site, so links stay valid for CLI clients
 * (install.sh) and never point at the API host.
 */
function signum_site_base_url(): string
{
    $host = trim((string)($_SERVER['HTTP_HOST'] ?? ''));
    if (!preg_match('/^[A-Za-z0-9.-]+(:\d+)?$/D', $host)) {
        return 'https://appbank.pw/signum';
    }
    $https = (!empty($_SERVER['HTTPS']) && strtolower((string)$_SERVER['HTTPS']) !== 'off')
        || strtolower((string)($_SERVER['HTTP_X_FORWARDED_PROTO'] ?? '')) === 'https';
    return ($https ? 'https://' : 'http://') . $host . '/signum';
}

