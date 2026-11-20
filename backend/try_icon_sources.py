                      
import json
import re
import sys
import urllib.request

sys.stdout.reconfigure(line_buffering=True)
UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
aid = "564177498"


def try_url(url: str, t: int = 25) -> str:
    print("TRY", url[:140], flush=True)
    try:
        req = urllib.request.Request(
            url,
            headers={
                "User-Agent": UA,
                "Accept": "application/json,text/html,*/*",
                "Origin": "https://apps.apple.com",
            },
        )
        with urllib.request.urlopen(req, timeout=t) as r:
            body = r.read().decode("utf-8", "replace")
            print("  OK", r.status, "len", len(body), "mzstatic=", ("mzstatic.com" in body), flush=True)
            icons = re.findall(
                r"https://is\d+-ssl\.mzstatic\.com/image/thumb/[^\"'\\s<>]+",
                body,
            )
            for i in icons[:8]:
                print("  ICON", i[:160], flush=True)
            return body
    except Exception as e:
        print("  FAIL", e, flush=True)
        return ""


urls = [
    f"https://itunes.apple.com/lookup?id={aid}",
    f"https://itunes.apple.com/lookup?id={aid}&entity=software",
    f"https://apps.apple.com/us/app/id{aid}",
    f"https://apps.apple.com/ru/app/id{aid}",
    f"https://amp-api.apps.apple.com/v1/catalog/us/apps/{aid}?platform=iphone",
    f"https://amp-api.apps.apple.com/v1/catalog/ru/apps/{aid}?platform=iphone",
    f"https://api.appfigures.com/v2/app/ios/{aid}",
    f"https://data.42matters.com/api/v2.0/ios/apps/lookup.json?id={aid}",
                                      
    f"https://images.appmagic.rocks/ios/{aid}",
    f"https://images.appmagic.rocks/?store=2&id={aid}",
                       
    f"https://www.similarweb.com/app/apple/{aid}/",
]

for u in urls:
    try_url(u)
    print("---", flush=True)
