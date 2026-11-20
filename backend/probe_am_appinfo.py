                      
import json
import re
import sys
import urllib.request
import urllib.error

sys.stdout.reconfigure(line_buffering=True)

TOKEN = "S5tgAvA8yFGByYWyXBoc-7Wv4YpQ29kE"
UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36"
COOKIE = (
    "dashly_device_guid=161f5bb5-a711-4bbe-a7fb-e0c5bddc85fd; "
    "dashly_uid=2066609493382267361; "
    "dashly_auth_token=user.2066609493382267361.7830-e4dd41f4662c0285e8163b41d6e.7d1895f9543ec78c882e2ad8693967b36a4225c692956c09; "
    "hubspotutk=5ace068022dd152547b0568f1b78b3ec; "
    "cf_clearance=QAwXyufztJ6L6vdKuW6zNiDaOPpkvUbhbbQe._UGM64-1771183652-1.2.1.1-Gzhj7Nw3tXtEkKBttkwxHlYuenFnNRq18zMYghCiPUSw7zbz5CDd.gr9CTnBx6rtAPmiT.VVoAiQKLLINiof3HNAbIMqRdCYDuUE390Hi9oGg9IYufYFoYK.92_1vmd19rUo3bv_QOjqazslCxwZYN4O2YsAZabjeV4fOEtI7T_DTh8fzGE03aN9Vv4ErVyJo.zYlxPietM1dooVOqArA7ZzzcF7MHtkc5oRHfvXjgw"
)

APP_ID = "564177498"
UNITED_ID = 99892               


def call(method: str, url: str, data=None):
    headers = {
        "accept": "application/json, text/plain, */*",
        "authorization": f"Bearer {TOKEN}",
        "content-language": "en",
        "accept-language": "ru,en;q=0.9",
        "origin": "https://appmagic.rocks",
        "referer": f"https://appmagic.rocks/iphone/vk-social-network-messenger/{APP_ID}?infoCountry=RU",
        "user-agent": UA,
        "cookie": COOKIE,
    }
    body = None
    if data is not None:
        headers["content-type"] = "application/json"
        body = json.dumps(data).encode()
    req = urllib.request.Request(url, data=body, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=40) as resp:
            raw = resp.read().decode("utf-8", "replace")
            return resp.status, raw
    except urllib.error.HTTPError as e:
        raw = e.read().decode("utf-8", "replace")
        return e.code, raw


def show(label, code, raw):
    print(f"\n=== {label} => {code} len={len(raw)} ===", flush=True)
    if raw.lstrip().startswith("<!"):
        print("  HTML page", flush=True)
        return
    print(f"  mzstatic={('mzstatic.com' in raw)} play={('play-lh' in raw)}", flush=True)
    try:
        data = json.loads(raw)
        print("  type", type(data).__name__, flush=True)
        if isinstance(data, dict):
            print("  keys", list(data.keys())[:40], flush=True)
        text = json.dumps(data, ensure_ascii=False)
        icons = re.findall(r"https://[^\"'\\s]+(?:mzstatic|play-lh|googleusercontent)[^\"'\\s]*", text)
        for i in icons[:15]:
            print("  ICON", i[:160], flush=True)
                            
        print("  sample", text[:500].replace("\n", " "), flush=True)
    except Exception as e:
        print("  raw", raw[:400], flush=True)


tests = [
    ("POST app-info id", "POST", "https://appmagic.rocks/api/v2/applications/app-info", {"id": APP_ID}),
    ("POST app-info store_application_id store2", "POST", "https://appmagic.rocks/api/v2/applications/app-info", {"store_application_id": APP_ID, "store": 2}),
    ("POST app-info storeApplicationId", "POST", "https://appmagic.rocks/api/v2/applications/app-info", {"storeApplicationId": APP_ID, "store": 2}),
    ("POST app-info united id", "POST", "https://appmagic.rocks/api/v2/applications/app-info", {"id": UNITED_ID}),
    ("POST app-info unitedApplicationId", "POST", "https://appmagic.rocks/api/v2/applications/app-info", {"unitedApplicationId": UNITED_ID}),
    ("GET app-info q", "GET", f"https://appmagic.rocks/api/v2/applications/app-info?id={APP_ID}&store=2", None),
    ("GET app-info store_application_id", "GET", f"https://appmagic.rocks/api/v2/applications/app-info?store_application_id={APP_ID}&store=2", None),
    ("POST image-thumbnail", "POST", "https://appmagic.rocks/api/v2/image-thumbnail", {"url": f"https://play-lh.googleusercontent.com/test", "size": 300}),
    ("GET image-thumbnail", "GET", f"https://appmagic.rocks/api/v2/image-thumbnail?url=https://is1-ssl.mzstatic.com/image/thumb/test/0x514h.jpg", None),
    ("POST united search-by-ids", "POST", "https://appmagic.rocks/api/v2/united-applications/search-by-ids", {"ids": [UNITED_ID]}),
    ("POST search applications", "POST", "https://appmagic.rocks/api/v2/search/applications", {"query": APP_ID, "store": 2}),
    ("GET search applications", "GET", f"https://appmagic.rocks/api/v2/search/applications?name={APP_ID}&store=2", None),
    ("POST applications history", "POST", "https://appmagic.rocks/api/v2/applications/history", {"store_application_id": APP_ID, "store": 2, "country": "RU"}),
    ("GET similarity (old)", "POST", "https://appmagic.rocks/api/v2/similarity/app-info", {"id": APP_ID}),
]

for label, method, url, data in tests:
    code, raw = call(method, url, data)
    show(label, code, raw)
