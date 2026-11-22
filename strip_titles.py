import re

with open('backend/signum/common.php', 'r', encoding='utf-8') as f:
    content = f.read()

def clean_title(m):
    title = m.group(1)
                                                         
    cleaned = re.sub(r'\s*\([^)]*\)', '', title).strip()
                                     
    cleaned = re.sub(r'\s*\([^)]*\)', '', cleaned).strip()
    return f"'title' => '{cleaned}'"

new_content = re.sub(r"'title' => '([^']+)'", clean_title, content)

with open('backend/signum/common.php', 'w', encoding='utf-8') as f:
    f.write(new_content)

print("Stripped all () from titles")

        
with open('backend/signum/common.php', 'r', encoding='utf-8') as f:
    c = f.read()
rem = len(re.findall(r"'title' => '[^']*\([^)]*\)[^']*'", c))
print(f"Remaining titles with (): {rem}")
