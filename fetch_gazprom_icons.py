import urllib.request
import json
import re
import time

def fetch_icon_by_id(app_id):
    """Query appmagic using name=ID (as instructed in PROMT.txt), return icon ending with 0x300h.png"""
    url = f'https://appmagic.rocks/api/v2/search?name={app_id}&limit=3'
    try:
        req = urllib.request.Request(url, headers={
            'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36'
        })
        with urllib.request.urlopen(req, timeout=25) as resp:
            data = json.loads(resp.read().decode('utf-8'))
            apps = data.get('applications', [])
            if apps:
                icon = apps[0].get('icon_url', '')
                if icon:
                    if '/0x' in icon:
                        base = icon.split('/0x')[0]
                        new_icon = base + '/0x300h.png'
                        return new_icon
                    else:
                        return re.sub(r'/[^/]+$', '/0x300h.png', icon)
    except Exception as e:
        print(f'  ERROR for {app_id}: {e}')
    return None

ids_titles = [
    ('1406492297', 'Газпром Банк'),
    ('6443581021', 'Millioner consulting'),
    ('6482298042', 'Спутник по вопросам'),
    ('6648796739', 'В балансе'),
    ('6738935433', 'Your Shopper'),
    ('6746754412', 'До вершины'),
    ('6739004924', 'CashHunter'),
    ('6756115455', 'GiftMate'),
    ('6755891115', 'Цифровой сейф'),
]

print("Fetching icons for Gazprom apps (by ID only)...")
results = {}
for appid, title in ids_titles:
    print(f"Looking up {title} ({appid}) ...")
    icon = fetch_icon_by_id(appid)
    results[appid] = icon
    if icon:
        print(f"  -> {icon}")
    else:
        print("  -> None")
    time.sleep(0.8)                        

print("\n=== RESULTS ===")
for appid, icon in results.items():
    print(f"{appid}: {icon}")

                 
with open('gazprom_icons.json', 'w', encoding='utf-8') as f:
    json.dump(results, f, indent=2, ensure_ascii=False)
print("\nSaved to gazprom_icons.json")
