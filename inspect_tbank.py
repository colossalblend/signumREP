import re

with open('backend/signum/common.php', 'r', encoding='utf-8') as f:
    content = f.read()

                     
m = re.search(r"'tbank' => .*?'versions' => \[(.*?)\]\s*,\s*'\]", content, re.DOTALL)
if not m:
    print('no tbank')
    exit()

versions = m.group(1)
entries = re.findall(r"'title' => '([^']+)'.*?'app_store_id' => '([^']*)'.*?'icon_url' => '([^']*)'", versions, re.DOTALL)

print('Current tbank apps with IDs:')
for title, appid, icon in entries:
    if appid.strip():
        is_generic = '685855ad' in icon or 'Placeholder.mill' in icon
        print(f'{title}: ID={appid}  generic={is_generic}')
        if not is_generic:
            print(f'   icon: {icon[:80]}...')
print(f'\nTotal with ID: {len([e for e in entries if e[1].strip()])}')
