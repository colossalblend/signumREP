import re

with open('backend/signum/common.php', 'r', encoding='utf-8') as f:
    content = f.read()

                            
gazprom_start = content.find("'id' => 'gazprom',")
if gazprom_start == -1:
    print("ERROR: gazprom not found")
    exit(1)

                     
versions_start = content.find("'versions' => [", gazprom_start)
if versions_start == -1:
    print("ERROR: versions not found")
    exit(1)

                                                         
search_area = content[versions_start:]
depth = 0
end_offset = 0
for i, char in enumerate(search_area):
    if char == '[':
        depth += 1
    elif char == ']':
        depth -= 1
        if depth == 0:
            end_offset = i + 1
            break

if end_offset == 0:
    print("ERROR: could not find end of versions")
    exit(1)

old_block = search_area[:end_offset]
print(f"Found versions block, length {len(old_block)}")

                                     
                       
                   
                                       
                         
                    

                                                                                
                                                                                        

                                                                   
version_regex = r'(\s+\[\s*\n(?:\s+\'[^\']+\' => [^\n]+,\s*\n)+?\s+\],\s*\n)'

versions_list = re.findall(version_regex, old_block, re.DOTALL)

print(f"Extracted {len(versions_list)} version entries")

if len(versions_list) != 9:
    print("Warning: did not extract exactly 9. Dumping first 300 chars of block:")
    print(old_block[:300])
                              
                                                   
    parts = re.split(r'(\s+\],\s*\n)', old_block)
                 
    temp = []
    for i in range(0, len(parts)-1, 2):
        if i+1 < len(parts):
            temp.append(parts[i] + parts[i+1])
    versions_list = temp
    print(f"Fallback extracted {len(versions_list)}")

if len(versions_list) == 0:
    print("Failed to parse versions")
    exit(1)

                  
reversed_versions = list(reversed(versions_list))

                           
                                       
prefix = old_block.split('\n', 1)[0] + '\n'                   
inner = ''.join(reversed_versions)
                                                  
suffix_match = re.search(r'(\s+\],\s*\n?)$', old_block)
suffix = suffix_match.group(1) if suffix_match else '            ],\n'

new_block = prefix + inner + suffix

                                                 
new_content = content[:versions_start] + new_block + content[versions_start + len(old_block):]

                                                                                       
                                            
gpos = new_content.find("'id' => 'gazprom',")
if gpos != -1:
    vs = new_content.find("'versions' => [", gpos)
              
    sa = new_content[vs:]
    d = 0
    eo = 0
    for ii, ch in enumerate(sa):
        if ch == '[': d += 1
        elif ch == ']':
            d -= 1
            if d == 0:
                eo = ii + 1
                break
    gblock = new_content[vs:vs+eo]

                            
    gblock = re.sub(r"'is_latest' => true,", "'is_latest' => false,", gblock)

                                                                      
                                        
    pattern = r"('app_store_id' => '1406492297',)(\s*'icon_url' => [^,]+,\s*'min_ios' => [^,]+,\s*)'is_latest' => false,"
    replacement = r"\1\2'is_latest' => true,"
    gblock = re.sub(pattern, replacement, gblock, flags=re.DOTALL)

    new_content = new_content[:vs] + gblock + new_content[vs+eo:]

            
with open('backend/signum/common.php', 'w', encoding='utf-8') as f:
    f.write(new_content)

print("SUCCESS: versions list reversed.")
print("Цифровой сейф is now first in the array (top).")
print("Газпром Банк is now last (bottom).")
print("is_latest kept only on the Газпром Банк entry.")
