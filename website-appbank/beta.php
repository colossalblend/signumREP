<?php
declare(strict_types=1);

session_start();

/**
 * signum/beta.php — заявки на закрытое β-тестирование Signum.
 *
 * Правила страницы — источник истины: iOS 17.6+, распространение
 * распространение тестовой сборки запрещено: нарушители исключаются
 * из группы тестирования и блокируются в канале.
 * две роли: покупатель и продавец.
 *
 * Заявки сохраняются в приватную папку ВНЕ веб-корня:
 *   <FTP-корень>/signum-private/beta-applications.jsonl
 * Локальный fallback для разработки: ./.local/beta-applications.jsonl
 * (с авто-создаваемым .htaccess «запретить всё»).
 */

const BETA_ROLES = ['buyer', 'seller'];

function signum_beta_storage_path(): string
{
    // __DIR__ = <home>/www/appbank.pw/signum → dirname(__DIR__, 3) = FTP-корень
    $root = dirname(__DIR__, 3);
    $private = $root . '/signum-private';
    if (is_dir($private) && is_writable($private)) {
        return $private . '/beta-applications.jsonl';
    }
    $local = __DIR__ . '/.local';
    if (!is_dir($local)) {
        @mkdir($local, 0710, true);
        @file_put_contents(
            $local . '/.htaccess',
            "Require all denied\n<IfModule !mod_authz_core.c>\nOrder deny,allow\nDeny from all\n</IfModule>\n"
        );
    }
    return $local . '/beta-applications.jsonl';
}

function signum_beta_rate_limit(string $key, int $cooldown): bool
{
    $head = dirname(signum_beta_storage_path());
    $stamp = $head . '/.rl-' . md5($key);
    if (is_file($stamp) && time() - (int) filemtime($stamp) < $cooldown) {
        return false;
    }
    @file_put_contents($stamp, "1");
    return true;
}

function signum_beta_ip_quota(string $ip, int $limit = 5, int $window = 3600): bool
{
    $path = dirname(signum_beta_storage_path()) . '/.quota-' . md5($ip);
    $now = time();
    $data = ['started_at' => $now, 'count' => 0];
    if (is_file($path)) {
        $stored = json_decode((string) @file_get_contents($path), true);
        if (is_array($stored) && (int) ($stored['started_at'] ?? 0) + $window > $now) {
            $data = [
                'started_at' => (int) $stored['started_at'],
                'count' => (int) ($stored['count'] ?? 0),
            ];
        }
    }
    if ($data['count'] >= $limit) {
        return false;
    }
    $data['count']++;
    @file_put_contents($path, json_encode($data), LOCK_EX);
    return true;
}

function signum_beta_csrf_token(): string
{
    if (empty($_SESSION['signum_beta_csrf'])) {
        $_SESSION['signum_beta_csrf'] = bin2hex(random_bytes(32));
    }
    return (string) $_SESSION['signum_beta_csrf'];
}

function signum_beta_recent_duplicate(string $tg, string $role): bool
{
    $path = signum_beta_storage_path();
    if (!is_file($path) || filesize($path) > 5 * 1024 * 1024) {
        return false;
    }
    $handle = @fopen($path, 'rb');
    if ($handle === false) {
        return false;
    }
    $needle = strtolower(ltrim($tg, '@'));
    $now = time();
    while (($line = fgets($handle)) !== false) {
        $row = json_decode($line, true);
        if (!is_array($row) || ($row['role'] ?? '') !== $role || strtolower(ltrim((string) ($row['tg'] ?? ''), '@')) !== $needle) {
            continue;
        }
        $created = strtotime((string) ($row['created_at'] ?? ''));
        if ($created !== false && $now - $created < 30 * 86400) {
            fclose($handle);
            return true;
        }
    }
    fclose($handle);
    return false;
}

/** @return array{ok:bool, errors:array<string,string>} */
function signum_beta_handle_post(): array
{
    $post = $_POST;
    $errors = [];

    if ((int) ($_SERVER['CONTENT_LENGTH'] ?? 0) > 32 * 1024) {
        return ['ok' => false, 'errors' => ['global' => 'Форма содержит слишком много данных.']];
    }

    $csrf = (string) ($post['csrf'] ?? '');
    if ($csrf === '' || empty($_SESSION['signum_beta_csrf']) || !hash_equals((string) $_SESSION['signum_beta_csrf'], $csrf)) {
        return ['ok' => false, 'errors' => ['global' => 'Сессия формы устарела. Обновите страницу и попробуйте снова.']];
    }
    $startedAt = (int) ($_SESSION['signum_beta_started_at'] ?? 0);
    if ($startedAt === 0 || time() - $startedAt < 3) {
        return ['ok' => false, 'errors' => ['global' => 'Заполните форму чуть внимательнее и попробуйте снова.']];
    }
    if (time() - $startedAt > 7200) {
        return ['ok' => false, 'errors' => ['global' => 'Форма устарела. Обновите страницу и заполните её заново.']];
    }

    if (trim((string) ($post['hp'] ?? '')) !== '') {
        return ['ok' => true, 'errors' => []]; // honeypot: молча «успех» без записи
    }

    $role = (string) ($post['role'] ?? '');
    $tg = trim((string) ($post['tg'] ?? ''));
    $agree = (string) ($post['agree'] ?? '');
    $iphone = trim((string) ($post['iphone'] ?? ''));
    $ios = trim((string) ($post['ios'] ?? ''));
    $os = (string) ($post['os'] ?? '');

    if (!in_array($role, BETA_ROLES, true)) {
        $errors['role'] = 'Выбирайте роль — покупатель или продавец.';
    }
    if (!preg_match('~^@?[A-Za-z][A-Za-z0-9_]{3,31}$~', $tg)) {
        $errors['tg'] = 'Ник в Telegram — латиницей/цифрами, 4–32 символа.';
    }
    if ($agree !== 'on') {
        $errors['agree'] = 'Примите правила — без этого в тест не берём.';
    }

    $buyerData = [];
    if ($role === 'buyer') {
        if ($iphone === '' || mb_strlen($iphone) > 40) {
            $errors['iphone'] = 'Уточните модель iPhone — например, iPhone 15 Pro.';
        }
        if (!preg_match('~^(\d{1,2})\.(\d{1,2})(?:\.\d+)?$~', $ios, $m)) {
            $errors['ios'] = 'Укажите версию iOS цифрами — например, 17.6.1.';
        } else {
            $major = (int) $m[1];
            $minor = (int) $m[2];
            $fresh = $major > 17 || ($major === 17 && $minor >= 6);
            if (!$fresh) {
                $errors['ios'] = 'Минимум iOS 17.6 — ниже тестовая сборка не подойдёт.';
            }
        }
        if (!in_array($os, ['windows', 'macos', 'linux'], true)) {
            $errors['os'] = 'Выберите операционную систему компьютера.';
        }
        $buyerData = ['iphone' => $iphone, 'ios' => $ios, 'os' => $os];
    }

    if (!$errors) {
        $ip = (string) ($_SERVER['REMOTE_ADDR'] ?? 'local');
        $key = $ip . '|' . mb_strtolower(ltrim($tg, '@'));
        if (!signum_beta_ip_quota($ip)) {
            return ['ok' => false, 'errors' => ['global' => 'С этого адреса достигнут лимит заявок. Попробуйте позже.']];
        }
        if (!signum_beta_rate_limit('ip:' . $ip, 30)) {
            return ['ok' => false, 'errors' => ['global' => 'Слишком много запросов. Подождите немного и повторите.']];
        }
        if (!signum_beta_rate_limit('pair:' . $key, 300)) {
            return ['ok' => false, 'errors' => ['global' => 'Заявка с этими данными уже недавно отправлялась.']];
        }
        if (signum_beta_recent_duplicate($tg, $role)) {
            return ['ok' => false, 'errors' => ['global' => 'Заявка с этим Telegram уже есть. Если данные изменились, свяжитесь с организаторами.']];
        }
        $record = [
            'created_at' => (new DateTimeImmutable('now', new DateTimeZone('UTC')))->format(DateTimeInterface::ATOM),
            'role' => $role,
            'tg' => '@' . ltrim($tg, '@'),
            'ua' => (string) ($_SERVER['HTTP_USER_AGENT'] ?? ''),
            'ip' => $ip,
        ];
        if ($role === 'buyer') {
            $record = $record + $buyerData;
        }
        $written = @file_put_contents(
            signum_beta_storage_path(),
            json_encode($record, JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES) . "\n",
            FILE_APPEND | LOCK_EX
        );
        if ($written === false) {
            return ['ok' => false, 'errors' => ['global' => 'Не удалось сохранить заявку. Попробуйте позже или свяжитесь с организаторами.']];
        }
        return ['ok' => true, 'errors' => []];
    }
    return ['ok' => false, 'errors' => $errors];
}

$isAjax = (($_SERVER['HTTP_X_REQUESTED_WITH'] ?? '') === 'fetch');
$httpMethod = ($_SERVER['REQUEST_METHOD'] ?? 'GET');

$errors = [];
$sent = false;
$csrfToken = signum_beta_csrf_token();
if ($httpMethod !== 'POST') {
    $_SESSION['signum_beta_started_at'] = time();
}

if ($httpMethod === 'POST') {
    $result = signum_beta_handle_post();
    if ($result['ok']) {
        if ($isAjax) {
            header('Content-Type: application/json; charset=UTF-8');
            echo json_encode(['ok' => true], JSON_UNESCAPED_UNICODE);
            exit;
        }
        http_response_code(303);
        header('Location: beta.php?sent=1');
        exit;
    }
    if ($isAjax) {
        http_response_code(422);
        header('Content-Type: application/json; charset=UTF-8');
        echo json_encode(['ok' => false, 'errors' => $result['errors']], JSON_UNESCAPED_UNICODE);
        exit;
    }
    $errors = $result['errors'];
}
if (($_GET['sent'] ?? '') === '1') {
    $sent = true;
}

// значения формы для повторного вывода при ошибках (fallback, когда нет JS)
$fNot = fn(string $k): string => htmlspecialchars((string) ($_POST[$k] ?? ''), ENT_QUOTES, 'UTF-8');
$role = (string) ($_POST['role'] ?? '');
function e(string $s): string
{
    return htmlspecialchars($s, ENT_QUOTES, 'UTF-8');
}
?>
<!doctype html>
<html lang="ru" dir="ltr">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
    <title>Signum Beta — заявка на тестирование</title>
    <meta name="description" content="Закрытое β-тестирование Signum: правила участия и заявка. iOS 17.6+, роли покупатель и продавец." />
    <meta name="robots" content="noindex, nofollow" />
    <meta name="theme-color" content="#0b0d13" />
    <meta name="color-scheme" content="dark" />
    <link rel="icon" type="image/png" sizes="32x32" href="assets/signum-icon-32.png" />
    <link rel="icon" type="image/png" sizes="64x64" href="assets/signum-icon-64.png" />
    <link rel="icon" type="image/png" href="assets/signum-icon.png" />
    <link rel="apple-touch-icon" href="assets/signum-icon-192.png" />
    <link rel="manifest" href="site.webmanifest" />
    <link rel="stylesheet" href="styles.css?v=20260925" />
    <style>
      .beta-wrap { max-width: 880px; margin: 0 auto; padding: 44px 16px 96px; }
      .beta-head { display: flex; align-items: center; gap: 14px; margin-bottom: 12px; }
      .beta-head img { width: 44px; height: 44px; border-radius: 12px; border: 1px solid var(--border-strong); }
      .beta-head a { color: var(--text-soft); font-size: 14px; text-decoration: none; }
      .beta-head a:hover { color: var(--text); }
      h1.beta-title { font-size: 34px; line-height: 1.15; margin: 12px 0 10px; }
      .beta-sub { color: var(--text-soft); font-size: 16px; max-width: 660px; line-height: 1.6; }
      .beta-chips { display: flex; flex-wrap: wrap; gap: 8px; margin: 18px 0 28px; }
      .beta-chip { border: 1px solid var(--border-strong); border-radius: var(--radius-pill); padding: 7px 14px; font-size: 13.5px; color: var(--text-soft); background: var(--surface); }
      .beta-card { background: rgba(18, 22, 32, 0.88); border: 1px solid var(--border); border-radius: 14px; padding: 26px; margin-bottom: 22px; box-shadow: 0 18px 50px rgba(0, 0, 0, 0.16); }
      .beta-card h2 { font-size: 21px; margin: 0 0 14px; }
      .beta-rules { border-color: rgba(248, 113, 113, 0.42); background: linear-gradient(180deg, rgba(248, 113, 113, 0.09), rgba(248, 113, 113, 0.02)), var(--surface); }
      .beta-rules ol { margin: 0; padding-left: 22px; color: var(--text-soft); line-height: 1.7; }
      .beta-rules li + li { margin-top: 10px; }
      .beta-rules b { color: var(--text); }
      .beta-red { color: var(--danger); font-weight: 700; }
      .beta-law { border-left: 3px solid var(--danger); background: rgba(248, 113, 113, 0.1); border-radius: 8px; padding: 14px 16px; margin-top: 14px; font-weight: 600; color: var(--text); }
      .beta-roles { display: grid; grid-template-columns: 1fr; gap: 12px; }
      @media (min-width: 720px) { .beta-roles { grid-template-columns: 1fr 1fr; } }
      .beta-label { display: block; font-size: 14px; color: var(--text-soft); margin: 18px 0 7px; }
      .beta-input, .beta-select { width: 100%; box-sizing: border-box; background: var(--bg-elevated); color: var(--text); border: 1px solid var(--border-strong); border-radius: var(--radius-sm); padding: 13px 15px; font: inherit; font-size: 15px; }
      .beta-input:focus, .beta-select:focus { outline: none; box-shadow: var(--focus); border-color: var(--brand); }
      .beta-input::placeholder { color: var(--faint); }
      .beta-input--invalid { border-color: var(--danger) !important; }
      .beta-error { color: var(--danger); font-size: 13px; margin-top: 6px; }
      .beta-form { display: grid; gap: 4px; background: transparent; border: 0; border-radius: 0; padding: 0; }
      .beta-note { color: var(--muted); font-size: 13px; margin-top: 6px; }
      .beta-check { display: flex; gap: 10px; align-items: flex-start; margin-top: 20px; color: var(--text-soft); font-size: 14.5px; line-height: 1.55; }
      .beta-check input { margin-top: 3px; accent-color: var(--brand); width: 16px; height: 16px; flex: none; }
      .beta-check a { color: var(--text); }
      .beta-role, .beta-os { display: block; margin: 0; cursor: pointer; padding: 18px 20px; transition: border-color 0.16s ease, background 0.16s ease, transform 0.16s ease; }
      .beta-role:hover, .beta-os:hover { border-color: var(--brand); transform: translateY(-1px); }
      .beta-role-inner, .beta-os-inner { display: flex; gap: 10px; align-items: flex-start; }
      .beta-os { padding: 14px 18px; }
      .beta-os .beta-os-inner { align-items: center; }
      .beta-ok { border-color: rgba(52, 211, 153, 0.45); background: linear-gradient(180deg, rgba(52, 211, 153, 0.1), rgba(52, 211, 153, 0.02)), var(--surface); }
      .beta-ok h2 { color: var(--success); }
      .beta-ok p { color: var(--text-soft); }
      .beta-submit { margin-top: 22px; display: inline-flex; justify-content: center; width: 100%; border: none; cursor: pointer; color: #fff; font: inherit; font-weight: 600; font-size: 15.5px; padding: 14px 26px; border-radius: 14px; background: linear-gradient(135deg, var(--brand), var(--brand-deep)); box-shadow: 0 12px 34px rgba(91, 108, 240, 0.35); transition: transform 0.16s var(--ease), box-shadow 0.16s var(--ease); }
      .beta-submit:hover { transform: translateY(-1px); box-shadow: 0 16px 40px rgba(91, 108, 240, 0.42); }
      .beta-submit[disabled] { opacity: 0.6; cursor: wait; transform: none; }
      .beta-global-error { border: 1px solid rgba(248, 113, 113, 0.5); border-radius: var(--radius-sm); padding: 12px 14px; color: var(--danger); margin-bottom: 14px; font-size: 14px; }
      .beta-footer { margin-top: 40px; color: var(--faint); font-size: 13px; text-align: center; }
      .beta-footer a { color: var(--text-soft); }
      .beta-row { display: grid; gap: 12px; }
      @media (min-width: 640px) { .beta-row { grid-template-columns: 1fr 1fr; } }
      .beta-hidden { position: absolute; left: -9999px; top: 0; }
      input[type="radio"], input[type="checkbox"] { accent-color: var(--brand); }
      @media (max-width: 600px) { h1.beta-title { font-size: 28px; } .beta-card { padding: 20px; } }
    </style>
  </head>
  <body>
    <div class="beta-wrap">
      <header class="beta-head">
        <img src="assets/signum-icon.png" alt="" width="44" height="44" />
        <div>
          <div style="font-weight: 700; letter-spacing: 0.2px;">Signum · β-тест</div>
          <a href="index.html">← Вернуться на главную</a>
        </div>
      </header>

<?php if ($sent): ?>
      <section class="beta-card beta-ok">
        <h2>Заявка принята</h2>
        <p>Мы рассматриваем заявки в порядке отправки. Если ответ положительный — напишем в Telegram.</p>
        <p>Тестовая сборка предназначена только для личного использования. За любое распространение — удаление из группы тестирования и блокировка в канале.</p>
        <p style="margin-top: 10px;"><a href="beta.php" style="color: var(--text); text-decoration: underline;">Обновить страницу</a></p>
      </section>
<?php else: ?>
      <h1 class="beta-title">Заявка на закрытый β-тест Signum</h1>
      <p class="beta-sub">Свободных мест немного: заявки просматриваем по мере поступления и отбираем по правилам ниже. Подойдёте — напишем в Telegram.</p>
      <div class="beta-chips">
        <span class="beta-chip">iOS 17.6+</span>
        <span class="beta-chip">Windows · Mac OS · Linux</span>
        <span class="beta-chip">2 роли: покупатель / продавец</span>
        <span class="beta-chip">Распространение запрещено</span>
      </div>
<?php if (e($errors['global'] ?? '') !== ''): ?><div class="beta-global-error"><?= e($errors['global']) ?></div><?php endif; ?>

      <section class="beta-card beta-rules" id="rules">
        <h2>Правила участия</h2>
        <ol>
          <li><b>iPhone с iOS 17.6 или новее.</b> Версию указывайте точной — 17.6, 17.6.1, 18.x, и так далее.</li>
          <li><b>Компьютер под рукой</b> — Windows, Mac OS или Linux: приложение Signum ставится на него, iPhone подключается к нему.</li>
          <li><b>Тестовая версия используется только для вашего личного теста.</b> Сборка может быть нестабильной — это нормально для беты.</li>
          <li>
            <span class="beta-red">Распространять тестовую версию запрещено.</span> Нельзя передавать установщик или ссылку на него, публиковать копию в чатах, на сайтах или в облачных хранилищах.
            <div class="beta-law">
              Любое распространение тестовой версии — удаление из группы тестирования и блокировка в канале.
            </div>
          </li>
          <li><b>Обратная связь — организаторам в Telegram.</b> Пишите о найденных ошибках и результатах тестирования в личные сообщения.</li>
        </ol>
      </section>

      <form class="beta-card beta-form" id="beta-form" method="post" action="beta.php" novalidate>
        <div class="beta-global-error" id="beta-status" hidden></div>
        <input type="hidden" name="csrf" value="<?= e($csrfToken) ?>" />
        <input type="text" name="hp" id="hp" class="beta-hidden" tabindex="-1" autocomplete="off" aria-hidden="true" />

        <h2>Анкета</h2>

        <div class="beta-roles">
          <label class="beta-card beta-role">
            <span class="beta-role-inner">
              <input type="radio" name="role" value="buyer" id="role-buyer"<?= $role === 'buyer' ? ' checked' : '' ?> />
              <span>
                <span style="display: block; font-weight: 700;">Покупатель</span>
                <span style="display: block; color: var(--text-soft); font-size: 14px; line-height: 1.5;">Устанавливаете приложения на свой iPhone и проверяете работу Signum.</span>
              </span>
            </span>
          </label>
          <label class="beta-card beta-role">
            <span class="beta-role-inner">
              <input type="radio" name="role" value="seller" id="role-seller"<?= $role === 'seller' ? ' checked' : '' ?> />
              <span>
                <span style="display: block; font-weight: 700;">Продавец</span>
                <span style="display: block; color: var(--text-soft); font-size: 14px; line-height: 1.5;">Устанавливаете приложения другим пользователям через Signum.</span>
              </span>
            </span>
          </label>
        </div>
        <div class="beta-error" data-err="role"><?= e($errors['role'] ?? '') ?></div>

        <label class="beta-label" for="tg">Ник в Telegram для связи</label>
        <input class="beta-input" name="tg" id="tg" type="text" maxlength="33" required placeholder="@nickname" value="<?= $fNot('tg') ?>" />
        <div class="beta-error" data-err="tg"><?= e($errors['tg'] ?? '') ?></div>
        <div class="beta-note">Ник нужен для связи с организаторами тестирования.</div>

        <div id="buyer-fields"<?= $role !== 'buyer' ? ' hidden' : '' ?>>
          <div class="beta-row">
            <div>
              <label class="beta-label" for="iphone">Модель iPhone</label>
              <input class="beta-input" name="iphone" id="iphone" type="text" maxlength="40" placeholder="iPhone 15 Pro" value="<?= $fNot('iphone') ?>" />
              <div class="beta-error" data-err="iphone"><?= e($errors['iphone'] ?? '') ?></div>
            </div>
            <div>
              <label class="beta-label" for="ios">Версия iOS</label>
              <input class="beta-input" name="ios" id="ios" type="text" inputmode="decimal" maxlength="12" placeholder="17.6.1" value="<?= $fNot('ios') ?>" />
              <div class="beta-error" data-err="ios"><?= e($errors['ios'] ?? '') ?></div>
            </div>
          </div>

          <div class="beta-label" style="margin-top: 18px;">Операционная система компьютера</div>
          <div class="beta-roles" style="row-gap: 8px;">
            <label class="beta-card beta-os"><span class="beta-os-inner"><input type="radio" name="os" value="windows"<?= ($_POST['os'] ?? '') === 'windows' ? ' checked' : '' ?> /><span style="font-weight: 600;">Windows</span></span></label>
            <label class="beta-card beta-os"><span class="beta-os-inner"><input type="radio" name="os" value="macos"<?= ($_POST['os'] ?? '') === 'macos' ? ' checked' : '' ?> /><span style="font-weight: 600;">Mac OS</span></span></label>
            <label class="beta-card beta-os"><span class="beta-os-inner"><input type="radio" name="os" value="linux"<?= ($_POST['os'] ?? '') === 'linux' ? ' checked' : '' ?> /><span style="font-weight: 600;">Linux</span></span></label>
          </div>
          <div class="beta-error" data-err="os"><?= e($errors['os'] ?? '') ?></div>
        </div>

        <label class="beta-check">
          <input type="checkbox" name="agree" id="agree"<?= ($_POST['agree'] ?? '') === 'on' ? ' checked' : '' ?> required />
          <span>Я прочитал правила и обязуюсь их соблюдать: тестовая версия — только для личного использования, <span class="beta-red">за любое распространение — удаление из группы тестирования и блокировка в канале.</span></span>
        </label>
        <div class="beta-error" data-err="agree"><?= e($errors['agree'] ?? '') ?></div>

        <button class="beta-submit" id="beta-submit" type="submit">Отправить заявку</button>
        <div class="beta-note" style="text-align: center;">Повторную заявку с теми же данными можно отправить через пять минут.</div>
      </form>
<?php endif; ?>

      <div class="beta-footer">
        Beta-программа Signum · сборка 1.2.1 (b41) · <a href="index.html">appbank.pw/signum</a>
      </div>
    </div>

    <script>
      (function () {
        "use strict";
        var form = document.getElementById("beta-form");
        if (!form) return;
        var statusBox = document.getElementById("beta-status");
        var buyerFields = document.getElementById("buyer-fields");
        var buyerRole = document.getElementById("role-buyer");
        var sellerRole = document.getElementById("role-seller");
        var submit = document.getElementById("beta-submit");
        var submitLabel = submit ? submit.textContent : "";

        function markError(field, message) {
          var slot = form.querySelector('[data-err="' + field + '"]');
          if (slot) slot.textContent = message || "";
          var input = form.querySelector('[name="' + field + '"]');
          if (input && input.classList) {
            if (message) { input.classList.add("beta-input--invalid"); }
            else { input.classList.remove("beta-input--invalid"); }
          }
        }

        function refreshVisibility() {
          if (buyerFields && buyerRole) buyerFields.hidden = !buyerRole.checked;
        }
        if (buyerRole && sellerRole) {
          buyerRole.addEventListener("change", refreshVisibility);
          sellerRole.addEventListener("change", refreshVisibility);
        }

        function scrollToFirstError(errors) {
          var keys = Object.keys(errors);
          for (var i = 0; i < keys.length; i++) {
            var slot = form.querySelector('[data-err="' + keys[i] + '"]');
            if (slot) {
              slot.scrollIntoView({ behavior: "smooth", block: "center" });
              break;
            }
          }
        }

        form.addEventListener("submit", function (event) {
          event.preventDefault();
          if (statusBox) statusBox.hidden = true;
          markError("role", "");
          markError("tg", "");
          markError("iphone", "");
          markError("ios", "");
          markError("os", "");
          markError("agree", "");

          var data = new FormData(form);
          if (submit) { submit.disabled = true; submit.textContent = "Отправляем…"; }

          fetch("beta.php", {
            method: "POST",
            body: data,
            headers: { "X-Requested-With": "fetch" },
            cache: "no-store"
          })
            .then(function (response) {
              return response.json().catch(function () { return null; });
            })
            .then(function (result) {
              if (result && result.ok) {
                form.innerHTML = "";
                var card = document.createElement("div");
                card.className = "beta-card beta-ok";
                card.innerHTML =
                  "<h2>Заявка принята</h2>" +
                  "<p>Если возьмём — напишем в Telegram.</p>" +
                  "<p style=\"margin-top:8px\">Тестовая сборка только для личного использования. Любое распространение = удаление из группы тестирования и блокировка в канале.</p>";
                form.appendChild(card);
                form.scrollIntoView({ behavior: "smooth", block: "center" });
                return;
              }
              var errors = (result && (result.errors || {})) || {};
              markError("role", errors.role || "");
              markError("tg", errors.tg || "");
              markError("iphone", errors.iphone || "");
              markError("ios", errors.ios || "");
              markError("os", errors.os || "");
              markError("agree", errors.agree || "");
              if (statusBox) {
                statusBox.textContent = errors.global || "Проверьте форму — остались незаполненные пункты.";
                statusBox.hidden = false;
              }
              if (!errors.global) scrollToFirstError(errors);
            })
            .catch(function () {
              if (statusBox) {
                statusBox.textContent = "Сеть недоступна — повторите попытку.";
                statusBox.hidden = false;
              }
            })
            .then(function () {
              if (submit) { submit.disabled = false; submit.textContent = submitLabel; }
            });
        });
      })();
    </script>
  </body>
</html>
