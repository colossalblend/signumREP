import re

with open('backend/signum/common.php', 'r', encoding='utf-8') as f:
    lines = f.readlines()

in_tbank = False
in_versions = False
current_title = None
current_id = None
current_icon = None
needing = []

for line in lines:
    if "'tbank' => [" in line:
        in_tbank = True
    if in_tbank and "'versions' => [" in line:
        in_versions = True
    if in_versions:
        if "'title' => '" in line:
            m = re.search(r"'title' => '([^']+)'", line)
            if m:
                current_title = m.group(1)
        if "'app_store_id' => '" in line:
            m = re.search(r"'app_store_id' => '([^']*)'", line)
            if m:
                current_id = m.group(1).strip()
        if "'icon_url' => '" in line:
            m = re.search(r"'icon_url' => '([^']*)'", line)
            if m:
                current_icon = m.group(1)
        if current_title and current_id and current_icon is not None:
            if current_id and (not current_icon or '685855ad' in current_icon or 'Placeholder.mill' in current_icon):
                needing.append((current_title, current_id))
            current_title = None
            current_id = None
            current_icon = None
        if '],' in line and current_title is None:                
            pass
    if in_tbank and "]," in line and "'tbank'" not in line and in_versions:
                   
        break

print('Apps needing proper icon (title | app_store_id):')
for t, i in needing:
    print(f'{t} | {i}')
print(f'\nTotal: {len(needing)}')
