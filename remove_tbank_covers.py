import re

with open('backend/signum/common.php', 'r', encoding='utf-8') as f:
    content = f.read()

                                  
versions_match = re.search(r"('versions' => \[\s*)(\[\s*'title' => 'Toastmas'.*?\],\s*)(\[\s*'title'.*?)(,\s*\]\s*,\s*)", content, re.DOTALL)

if not versions_match:
    print("Could not find the versions block")
    exit(1)

                                                        
new_block = versions_match.group(1) + versions_match.group(2) + "\n            ],"

                          
new_content = content[:versions_match.start()] + new_block + content[versions_match.end():]

with open('backend/signum/common.php', 'w', encoding='utf-8') as f:
    f.write(new_content)

print("Successfully removed all tbank covers except Toastmas")

                                        
with open('backend/signum/common.php', 'r', encoding='utf-8') as f:
    c = f.read()
titles = re.findall(r"'title' => '([^']+)'", re.search(r"'tbank' => .*?'versions' => \[(.*?)\]", c, re.DOTALL).group(1))
print(f"Remaining tbank versions: {titles}")
