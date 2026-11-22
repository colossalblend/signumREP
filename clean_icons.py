import re

with open('backend/signum/common.php', 'r', encoding='utf-8') as f:
    content = f.read()

generic = "https://is1-ssl.mzstatic.com/image/thumb/PurpleSource211/v4/68/58/55/685855ad-6f7c-2837-989b-08d686ea0ece/Placeholder.mill/0x300h.png"

                                      
                                                   
toastmas_pattern = r"('app_store_id' => '6774629936',\s*\n\s*'icon_url' => ')" + re.escape(generic) + r"(',)"
content = re.sub(toastmas_pattern, r"\1__TOASTMAS_ICON__\2", content)

                                              
icon_line_pattern = r"\s*'icon_url' => '" + re.escape(generic) + r"',\n"
content = re.sub(icon_line_pattern, "", content)

                  
content = content.replace("__TOASTMAS_ICON__", generic)

with open('backend/signum/common.php', 'w', encoding='utf-8') as f:
    f.write(content)

print("Icons cleaned: generic placeholder removed from all except Toastmas")

                 
with open('backend/signum/common.php', 'r', encoding='utf-8') as f:
    c = f.read()
count = c.count(generic)
print(f"Remaining occurrences of generic icon: {count}")
