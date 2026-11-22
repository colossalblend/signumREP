import re

with open('backend/signum/common.php', 'r', encoding='utf-8') as f:
    content = f.read()

                  
tbank_start = content.find("'tbank' => [")
if tbank_start == -1:
    print("no tbank")
    exit()

tbank_section = content[tbank_start:tbank_start+20000]

               
entries = []
lines = tbank_section.split('\n')
i = 0
while i < len(lines):
    line = lines[i]
    if "'title' => '" in line:
        title_m = re.search(r"'title' => '([^']+)'", line)
        if title_m:
            title = title_m.group(1)
            appid = ''
            icon = ''
            j = i + 1
            while j < len(lines) and '],' not in lines[j]:
                if "'app_store_id' => '" in lines[j]:
                    appid_m = re.search(r"'app_store_id' => '([^']*)'", lines[j])
                    if appid_m:
                        appid = appid_m.group(1)
                if "'icon_url' => '" in lines[j]:
                    icon_m = re.search(r"'icon_url' => '([^']*)'", lines[j])
                    if icon_m:
                        icon = icon_m.group(1)
                j += 1
            if appid.strip():
                entries.append({'title': title, 'appid': appid, 'icon': icon})
            i = j
    i += 1

print('tbank apps with app_store_id:')
for e in entries:
    is_generic = '685855ad' in e['icon'] or 'Placeholder.mill' in e['icon'] or not e['icon'].strip()
    print(f"  {e['title']}: ID={e['appid']}  needs_good_icon={is_generic}")
print(f"\nTotal needing lookup: {sum(1 for e in entries if '685855ad' in e['icon'] or 'Placeholder.mill' in e['icon'] or not e['icon'].strip())}")
