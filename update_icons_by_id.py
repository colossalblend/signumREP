import re
import urllib.request
import json
import time

GENERIC = '685855ad-6f7c-2837-989b-08d686ea0ece/Placeholder.mill/0x300h.png'

def fetch_icon_by_id(app_id):
    """Query appmagic using name=ID as per promt.txt method, return icon as 0x300h.png or None"""
    url = f'https://appmagic.rocks/api/v2/search?name={app_id}&limit=3'
    try:
        req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
        with urllib.request.urlopen(req, timeout=15) as resp:
            data = json.loads(resp.read().decode('utf-8'))
            apps = data.get('applications', [])
            if apps:
                app = apps[0]
                icon = app.get('icon_url', '')
                if icon:
                    if '/0x' in icon:
                        base = icon.split('/0x')[0]
                        return base + '/0x300h.png'
                    else:
                                          
                        return re.sub(r'/[^/]+$', '/0x300h.png', icon)
    except Exception as e:
        print(f'  ERROR for {app_id}: {e}')
    return None

def update_icon_in_php(content, title, new_icon):
    """Find the entry for title and set/insert icon_url with new_icon"""
                                                                    
                                                                                                                                                          
    title_esc = re.escape(title)
    
                                                                            
    pattern = rf\"('title' => '{title_esc}'(?:.|\n)*?'app_store_id' => '[^']*',)\s*'icon_url' => '[^']*',\"
    def repl(m):
        return m.group(1) + f\"\\n                    'icon_url' => '{new_icon}',\\n\"
    new_content, n = re.subn(pattern, repl, content, flags=re.DOTALL)
    if n > 0:
        return new_content
    
    pattern2 = rf\"('title' => '{title_esc}'(?:.|\n)*?'app_store_id' => '[^']*',)(\s*)('min_ios' =>) \"
    def repl2(m):
        return m.group(1) + f\"\\n                    'icon_url' => '{new_icon}',\\n                    \" + m.group(2) + m.group(3)
    new_content, n = re.subn(pattern2, repl2, content, flags=re.DOTALL)
    if n > 0:
        return new_content
    
    return content

with open('backend/signum/common.php', 'r', encoding='utf-8') as f:
    content = f.read()

tbank_start = content.find(\"'tbank' => [\")
tbank = content[tbank_start:tbank_start + 30000]

needing = []
for entry in re.split(r'\n\s+\],\s*\n\s+\[', tbank):
    title_m = re.search(r\"'title' => '([^']+)'\", entry)
    id_m = re.search(r\"'app_store_id' => '([^']*)'\", entry)
    icon_m = re.search(r\"'icon_url' => '([^']*)'\", entry)
    if title_m and id_m:
        title = title_m.group(1)
        appid = id_m.group(1).strip()
        icon = icon_m.group(1) if icon_m else ''
        if appid and (not icon or GENERIC in icon or 'Placeholder.mill' in icon):
            needing.append((title, appid))

print(f'Found {len(needing)} apps needing icon lookup/update')

updated = 0
for title, appid in needing:
    print(f'Looking up {title} ({appid}) ...')
    icon = fetch_icon_by_id(appid)
    if icon:
        print(f'  Got: {icon}')
        content = update_icon_in_php(content, title, icon)
        updated += 1
    else:
        print('  No icon found')
    time.sleep(0.6)

with open('backend/signum/common.php', 'w', encoding='utf-8') as f:
    f.write(content)

print(f'\nUpdated {updated} icons.')
' 