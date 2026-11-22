import urllib.request
import json
import re
import time

def fetch_from_appmagic(app_id):
    url = f'https://appmagic.rocks/api/v2/search?name={app_id}&limit=5'
    try:
        req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
        with urllib.request.urlopen(req, timeout=25) as resp:
            data = json.loads(resp.read().decode('utf-8'))
            for app in data.get('applications', []):
                if str(app_id) in [str(s) for s in app.get('store_application_ids', [])]:
                    icon = app.get('icon_url', '')
                    if icon and 'play.google' not in icon and 'play-lh' not in icon:
                        return normalize(icon)
    except Exception as e:
        print('  appmagic err', e)
    return None

def fetch_from_itunes(app_id):
    """Fallback: use official Apple lookup to get artwork and make 300h style"""
    url = f'https://itunes.apple.com/lookup?id={app_id}&country=ru'
    try:
        req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
        with urllib.request.urlopen(req, timeout=25) as resp:
            data = json.loads(resp.read().decode('utf-8'))
            results = data.get('results', [])
            if results:
                artwork = results[0].get('artworkUrl512') or results[0].get('artworkUrl100') or results[0].get('artworkUrl60')
                if artwork:
                                                                                                                                 
                                                                                
                                                       
                    if '/source/' in artwork:
                        base = artwork.split('/source/')[0]
                        return base + '/0x300h.png'
                                    
                    if '/512x512' in artwork or '.jpg' in artwork.lower():
                        base = re.sub(r'/\d+x\d+[^/]*\.[^/]+$', '', artwork)
                        if '/image/thumb/' in base:
                            return base + '/0x300h.png'
                                                                    
                    return normalize(artwork)
    except Exception as e:
        print('  itunes err', e)
    return None

def normalize(icon):
    if not icon:
        return None
    if '/0x' in icon:
        base = icon.split('/0x')[0]
        return base + '/0x300h.png'
    return re.sub(r'/[^/]+$', '/0x300h.png', icon)

missing = ['1406492297', '6755891115']

print("Fetching missing Gazprom icons with fallbacks...")
for aid in missing:
    print(f"{aid}:")
    icon = fetch_from_appmagic(aid)
    if not icon or 'play' in (icon or ''):
        print("  appmagic gave bad, trying itunes...")
        icon = fetch_from_itunes(aid)
    print(f"  -> {icon}")
    time.sleep(1)
