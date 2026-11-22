import urllib.request
import json
import re
import time

def fetch_icon_by_id(app_id, debug=False):
    url = f'https://appmagic.rocks/api/v2/search?name={app_id}&limit=5'
    try:
        req = urllib.request.Request(url, headers={
            'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36'
        })
        with urllib.request.urlopen(req, timeout=30) as resp:
            raw = resp.read().decode('utf-8')
            data = json.loads(raw)
            apps = data.get('applications', [])
            if debug:
                print(f"  Raw count: {len(apps)}")
                if apps:
                    print(f"  First name: {apps[0].get('name')}")
                    print(f"  First store ids: {apps[0].get('store_application_ids')}")
            if apps:
                                                                             
                for app in apps:
                    store_ids = app.get('store_application_ids', [])
                    if str(app_id) in [str(s) for s in store_ids]:
                        icon = app.get('icon_url', '')
                        if icon:
                            return normalize_icon(icon)
                                   
                icon = apps[0].get('icon_url', '')
                if icon:
                    return normalize_icon(icon)
    except Exception as e:
        print(f'  ERROR for {app_id}: {e}')
    return None

def normalize_icon(icon):
    if '/0x' in icon:
        base = icon.split('/0x')[0]
        return base + '/0x300h.png'
    else:
        return re.sub(r'/[^/]+$', '/0x300h.png', icon)

problem_ids = ['1406492297', '6443581021', '6755891115']

print("Retrying problematic Gazprom IDs...")
for aid in problem_ids:
    print(f"Looking up {aid} ...")
    icon = fetch_icon_by_id(aid, debug=True)
    print(f"  -> {icon}")
    time.sleep(1)
