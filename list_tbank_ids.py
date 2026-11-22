import re

with open('backend/signum/common.php', 'r', encoding='utf-8') as f:
    content = f.read()

tbank_start = content.find("'tbank' => [")
tbank = content[tbank_start : tbank_start + 30000]

                                       
pairs = []
current_title = None
for line in tbank.splitlines():
    if "'title' => '" in line:
        m = re.search(r"'title' => '([^']+)'", line)
        if m:
            current_title = m.group(1)
    if current_title and "'app_store_id' => '" in line:
        m = re.search(r"'app_store_id' => '([^']*)'", line)
        if m:
            appid = m.group(1).strip()
            if appid:
                pairs.append((current_title, appid))
            current_title = None

print("tbank apps with numeric app_store_id:")
for title, appid in pairs:
    print(f"  {title} | {appid}")
print(f"\nTotal: {len(pairs)}")
