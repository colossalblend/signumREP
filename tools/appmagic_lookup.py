                      
"""
Helper script to lookup apps on appmagic.rocks API.
Usage:
  python tools/appmagic_lookup.py "App Name"
  python tools/appmagic_lookup.py --id 6757754381   (lookup by store_application_id)

It will print name, IDs, and icon as 0x300h.png (or 0x500h for compatibility).

Now supports direct ID lookup as requested.
"""

import sys
import urllib.parse
import urllib.request
import json

def lookup(name):
    encoded = urllib.parse.quote(name)
    url = f"https://appmagic.rocks/api/v2/search?name={encoded}&limit=5"
    try:
        req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
        with urllib.request.urlopen(req, timeout=15) as response:
            data = json.loads(response.read().decode('utf-8'))
            apps = data.get('applications', [])
            if not apps:
                print(f"No results for: {name}")
                return
            for app in apps:
                print(f"Name: {app.get('name')}")
                ids = app.get('store_application_ids', [])
                print(f"App Store IDs: {ids}")
                icon = app.get('icon_url', '')
                if icon and '/0x' in icon:
                    base = icon.split('/0x')[0]
                    icon_300 = f"{base}/0x300h.png"
                    print(f"Icon URL (0x300h.png): {icon_300}")
                else:
                    print(f"Icon URL: {icon}")
                print("---")
    except Exception as e:
        print(f"Error fetching {name}: {e}")

def lookup_by_id(store_id):
    url = f"https://appmagic.rocks/api/v2/search?store_application_id={store_id}&limit=3"
    try:
        req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
        with urllib.request.urlopen(req, timeout=15) as response:
            data = json.loads(response.read().decode('utf-8'))
            apps = data.get('applications', [])
            if not apps:
                print(f"No results for store id: {store_id}")
                return
            for app in apps:
                print(f"Name: {app.get('name')}")
                ids = app.get('store_application_ids', [])
                print(f"App Store IDs: {ids}")
                icon = app.get('icon_url', '')
                if icon and '/0x' in icon:
                    base = icon.split('/0x')[0]
                    icon_300 = f"{base}/0x300h.png"
                    print(f"Icon URL (0x300h.png): {icon_300}")
                else:
                    print(f"Icon URL: {icon}")
                print("---")
    except Exception as e:
        print(f"Error fetching id {store_id}: {e}")

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage:")
        print("  python tools/appmagic_lookup.py \"App Name\"")
        print("  python tools/appmagic_lookup.py --id 6757754381")
        sys.exit(1)
    if sys.argv[1] == '--id' and len(sys.argv) > 2:
        lookup_by_id(sys.argv[2])
    else:
        lookup(sys.argv[1])
