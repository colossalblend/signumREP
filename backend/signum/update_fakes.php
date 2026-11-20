<?php
declare(strict_types=1);

$sourceFile = __DIR__ . '/common.php';
$source = file_get_contents($sourceFile);

                                                                         
                                                                     

require $sourceFile;
$catalog = signum_catalog_apps();
$tbankVersions = $catalog['tbank']['versions'];

                        
function makeFakeBundle(string $title): string {
    $manual = [
        'Смарт Т' => 'app.smart.t',
        'The Birds Voices' => 'app.thebirds.voices',
        'U Wheels' => 'app.u.wheels',
        'Т-Б' => 'app.t.b',
        'Т-Старт' => 'app.t.start',
        'Т-Баланс' => 'app.t.balance',
        'Т-Помощь' => 'app.t.help',
        'Т-Защита' => 'app.t.guard',
        'T-Bank' => 'app.t.bank',
        'Тинькофф' => 'app.tinkoff',
        'Toastmas' => 'app.toastmas',
    ];
    if (isset($manual[$title])) return $manual[$title];

    $slug = mb_strtolower($title);
    $slug = preg_replace('/[ё]/u', 'e', $slug);
    $slug = preg_replace('/[\s\-–_]+/u', '.', $slug);
    $slug = preg_replace('/[^a-z0-9.]/u', '', $slug);
    $slug = preg_replace('/\.+/', '.', $slug);
    $slug = trim($slug, '.');
    return 'app.' . ($slug ?: 'item');
}

                                                   
$verMajor = 7;
$verMinor = 35;
$verPatch = 9;

$updatedVersions = 0;
$updatedBundles = 0;

foreach ($tbankVersions as &$ver) {
    $title = $ver['title'] ?? 'Item';
    $oldVersion = $ver['version'] ?? '';
    $oldBundle = $ver['bundle_id'] ?? '';

                               
    $newBundle = makeFakeBundle($title);
    if ($oldBundle !== $newBundle) {
        $ver['bundle_id'] = $newBundle;
        $updatedBundles++;
    }

                      
    if ($title === 'Toastmas') {
        $ver['version'] = '7.36.1';
        if ($oldVersion !== '7.36.1') $updatedVersions++;
        continue;
    }

    if ($oldVersion === '7.36.1') {
        $newVer = $verMajor . '.' . $verMinor . '.' . $verPatch;
        $ver['version'] = $newVer;
        $updatedVersions++;

        $verPatch--;
        if ($verPatch < 0) {
            $verPatch = 9;
            $verMinor--;
        }
    }
}
unset($ver);

                                                                    
$versionsLiteral = var_export($tbankVersions, true);

                                               
                                                       

$pattern = "/('tbank' => \[.*?'versions' => )\s*\[.*?\],\s*(\n\s*'id' => 'tbank'|  \],)/s";

echo "Updated versions: $updatedVersions\n";
echo "Updated bundles: $updatedBundles\n";

                                                                                             
                                   
$searchForStart = "'id' => 'tbank',";
$startPos = strpos($source, $searchForStart);
if ($startPos === false) {
    die("Could not locate tbank section");
}

                                          
$versionsStartStr = "'versions' => [";
$versionsPos = strpos($source, $versionsStartStr, $startPos);
if ($versionsPos === false) {
    die("Could not locate versions");
}

                                                              
$searchFrom = substr($source, $versionsPos);
$depth = 0;
$endOffset = 0;
$len = strlen($searchFrom);
for ($i = 0; $i < $len; $i++) {
    $ch = $searchFrom[$i];
    if ($ch === '[') $depth++;
    elseif ($ch === ']') {
        $depth--;
        if ($depth === 0) {
            $endOffset = $i + 1;
            break;
        }
    }
}

if ($endOffset === 0) die("Could not find end of versions array");

                    
$oldVersionsBlock = substr($searchFrom, 0, $endOffset);

                  
$newVersionsBlock = "'versions' => " . $versionsLiteral . ",";

                    
$newSource = substr_replace($source, $newVersionsBlock, $versionsPos, strlen($oldVersionsBlock));

                   
$backup = $sourceFile . '.bak.' . date('YmdHis');
copy($sourceFile, $backup);
file_put_contents($sourceFile, $newSource);

echo "✅ Updated common.php\n";
echo "Backup saved to $backup\n";

                     
require $sourceFile;
$newCatalog = signum_catalog_apps();
$newTbank = $newCatalog['tbank']['versions'];
echo "\nVerification - Toastmas version: " . $newTbank[0]['version'] . "\n";
echo "Sample bundles:\n";
echo "  Toastmas: " . $newTbank[0]['bundle_id'] . "\n";
echo "  Смарт Т: " . $newTbank[63]['bundle_id'] . "\n";
echo "  The Birds Voices: " . $newTbank[48]['bundle_id'] . "\n";
echo "\nNumber of 7.36.1 left: " . count(array_filter($newTbank, fn($v) => ($v['version'] ?? '') === '7.36.1')) . "\n";
