import re

with open('backend/signum/common.php', 'r', encoding='utf-8') as f:
    content = f.read()

                                                                                                    
                                 
pattern = r"('app_store_id' => '[^']+'),(\s+)'min_ios'"
replacement = r"\1,\n                    'min_ios'"
content = re.sub(pattern, replacement, content)

with open('backend/signum/common.php', 'w', encoding='utf-8') as f:
    f.write(content)

print("Formatting cleaned up")
