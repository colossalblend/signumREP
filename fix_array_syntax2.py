import re

with open('backend/signum/common.php', 'r', encoding='utf-8') as f:
    content = f.read()

ts = content.find("'tbank' => [")
vs = content.find("'versions' => [", ts)
ve = content.find("    ];", ts)
if ve < 0:
    ve = ts + 30000

vblock = content[vs:ve]
pre = content[:vs]
post = content[ve:]

                                             
title_matches = list(re.finditer(r"'title' => '([^']+)'", vblock))

print(f"Found {len(title_matches)} title declarations")

entries = []
for idx, match in enumerate(title_matches):
    title = match.group(1)
    tpos = match.start()

                                                                                                    
                                                                                                  
    start = vblock.rfind('[', 0, tpos)
                                                         
    while start > 0:
                                                             
        if vblock.rfind('[', start+1, tpos) == -1:
            break
        start = vblock.rfind('[', 0, start)
    if start < 0:
        continue

                                                                   
    balance = 0
    end = start
    for j in range(start, len(vblock)):
        ch = vblock[j]
        if ch == '[':
            balance += 1
        elif ch == ']':
            balance -= 1
            if balance == 0:
                end = j + 1
                break

    block = vblock[start:end].strip()

                            
    if not block.endswith('],'):
        block = block.rstrip().rstrip(']').rstrip(',') + '],'

    entries.append((title, block))

print(f"Extracted {len(entries)} blocks")

               
indent = '                    '
bindent = '                '
new_v = "'versions' => [\n"
for title, block in entries:
                                                    
    get = lambda k, d='': (m.group(1) if (m := re.search(rf"'{k}' => '([^']*)'", block)) else d)
    get_bare = lambda k, d='': (m.group(1).strip() if (m := re.search(rf"'{k}' => ([^,\n]+)", block)) else d)

    new_v += bindent + '[\n'
    new_v += f"{indent}'title' => '{title}',\n"
    new_v += f"{indent}'description' => '{get('description')}', \n"
    new_v += f"{indent}'version' => '{get('version', '7.36.1')}', \n"
    new_v += f"{indent}'bundle_id' => '{get('bundle_id', 'NONE')}', \n"
    new_v += f"{indent}'app_store_id' => '{get('app_store_id')}', \n"
    icon = get('icon_url')
    if icon:
        new_v += f"{indent}'icon_url' => '{icon}', \n"
    new_v += f"{indent}'min_ios' => '{get('min_ios', '15.0')}', \n"
    new_v += f"{indent}'is_latest' => {get_bare('is_latest', 'false')}, \n"
    new_v += f"{indent}'source' => '{get('source', 'removed')}', \n"
    new_v += f"{indent}'in_store' => {get_bare('in_store', 'false')}, \n"
    new_v += bindent + '],\n'

new_v += '            ],\n        '

new_content = pre + new_v + post

with open('backend/signum/common.php', 'w', encoding='utf-8') as f:
    f.write(new_content)

print('Rebuilt cleanly.')

with open('backend/signum/common.php', 'r', encoding='utf-8') as f:
    c = f.read()
ts2 = c.find("'tbank' => [")
titles2 = re.findall(r"'title' => '([^']+)'", c[ts2:ts2+30000])
print('Titles after fix:', len(titles2))
print('Last few:', titles2[-5:])
