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

                                                                                         
                                                                        

title_pos = [m.start() for m in re.finditer(r"'title' => '([^']+)'", vblock)]

entries = []
for i, pos in enumerate(title_pos):
                                      
    open_br = vblock.rfind('[', 0, pos)
    if open_br < 0:
        continue
                                                                                                  
                                                   
    if i + 1 < len(title_pos):
        end_pos = title_pos[i+1]
    else:
        end_pos = len(vblock)
                                     
    sub = vblock[open_br:end_pos]
                                            
    m = re.search(r"'in_store' => [^,]+,\s*\n?\s*\]", sub)
    if m:
        entry_text = sub[: m.end()]
    else:
        entry_text = sub.rstrip().rstrip(',').rstrip() + '\n                ]'
    entries.append(entry_text.strip())

print(f'Extracted {len(entries)} version blocks')

                                   
indent = '                    '
b_indent = '                '
new_versions = "'versions' => [\n"
for i, entry in enumerate(entries):
                                                                              
                                                       
    title_m = re.search(r"'title' => '([^']*)'", entry)
    title = title_m.group(1) if title_m else 'Unknown'
    desc_m = re.search(r"'description' => '([^']*)'", entry)
    ver_m = re.search(r"'version' => '([^']*)'", entry)
    bundle_m = re.search(r"'bundle_id' => '([^']*)'", entry)
    aid_m = re.search(r"'app_store_id' => '([^']*)'", entry)
    icon_m = re.search(r"'icon_url' => '([^']*)'", entry)
    min_m = re.search(r"'min_ios' => '([^']*)'", entry)
    latest_m = re.search(r"'is_latest' => ([^,\s]+)", entry)
    source_m = re.search(r"'source' => '([^']*)'", entry)
    store_m = re.search(r"'in_store' => ([^,\s]+)", entry)

    new_versions += b_indent + '[\n'
    new_versions += f"{indent}'title' => '{title}',\n"
    new_versions += f"{indent}'description' => '{desc_m.group(1) if desc_m else ''}',\n"
    new_versions += f"{indent}'version' => '{ver_m.group(1) if ver_m else '7.36.1'}',\n"
    new_versions += f"{indent}'bundle_id' => '{bundle_m.group(1) if bundle_m else 'NONE'}',\n"
    new_versions += f"{indent}'app_store_id' => '{aid_m.group(1) if aid_m else ''}',\n"
    if icon_m:
        new_versions += f"{indent}'icon_url' => '{icon_m.group(1)}',\n"
    new_versions += f"{indent}'min_ios' => '{min_m.group(1) if min_m else '15.0'}',\n"
    new_versions += f"{indent}'is_latest' => {latest_m.group(1) if latest_m else 'false'},\n"
    new_versions += f"{indent}'source' => '{source_m.group(1) if source_m else 'removed'}',\n"
    new_versions += f"{indent}'in_store' => {store_m.group(1) if store_m else 'false'},\n"
    new_versions += b_indent + '],\n'
new_versions += '            ],\n        '

new_c = pre + new_versions + post

with open('backend/signum/common.php', 'w', encoding='utf-8') as f:
    f.write(new_c)

print('Fixed tbank versions array formatting and structure.')

              
with open('backend/signum/common.php', 'r', encoding='utf-8') as f:
    c = f.read()
ts2 = c.find("'tbank' => [")
titles2 = re.findall(r"'title' => '([^']+)'", c[ts2:ts2+30000])
print('Now titles in tbank:', len(titles2))
print('Last title:', titles2[-1] if titles2 else None)
