import re

                                                                   
ICON_MAP = {
    '1406492297': None,                                       
    '6443581021': 'https://is1-ssl.mzstatic.com/image/thumb/Purple126/v4/0b/f8/8e/0bf88e66-5929-1f75-de06-a52a5349df1f/AppIcon-0-1x_U007emarketing-0-5-0-85-220.png/0x300h.png',
    '6482298042': 'https://is1-ssl.mzstatic.com/image/thumb/Purple211/v4/fd/b7/15/fdb71599-e88c-b8a6-139b-f3a47b2dd0ce/AppIcon-0-0-1x_U007emarketing-0-5-0-85-220.png/0x300h.png',
    '6648796739': 'https://is1-ssl.mzstatic.com/image/thumb/Purple211/v4/a0/ac/28/a0ac2809-8e04-2643-f8d9-707e24af0112/AppIcon-0-0-1x_U007ephone-0-10-0-85-220.png/0x300h.png',
    '6738935433': 'https://is1-ssl.mzstatic.com/image/thumb/Purple221/v4/95/4e/62/954e6282-b963-7c59-9f0b-c98bcde13220/AppIcon-0-0-1x_U007emarketing-0-7-0-85-220.png/0x300h.png',
    '6746754412': 'https://is1-ssl.mzstatic.com/image/thumb/Purple221/v4/6a/fb/92/6afb9257-c3e3-66c5-7975-fac113f308f6/AppIcon-0-0-1x_U007emarketing-0-8-0-85-220.png/0x300h.png',
    '6739004924': 'https://is1-ssl.mzstatic.com/image/thumb/Purple221/v4/9d/9b/29/9d9b29be-5868-d95f-44d2-c9ba68bdc35f/AppIcon-0-0-1x_U007emarketing-0-8-0-85-220.png/0x300h.png',
    '6756115455': 'https://is1-ssl.mzstatic.com/image/thumb/Purple211/v4/66/d3/7d/66d37d73-6e2a-7c3d-b602-24aca683b25f/AppIcon-0-0-1x_U007emarketing-0-8-0-85-220.png/0x300h.png',
    '6755891115': None,                           
}

def update_icons(content):
    updated = 0
    for appid, new_icon in ICON_MAP.items():
        if not new_icon:
            continue
                                                                                        
                                                                     
                                                                         
        pattern = rf"('app_store_id' => '{re.escape(appid)}',)\s*'icon_url' => '[^']*',"
        replacement = rf"\1\n                    'icon_url' => '{new_icon}',"
        
        new_content, count = re.subn(pattern, replacement, content, flags=re.DOTALL)
        if count > 0:
            content = new_content
            updated += count
            print(f"Updated icon for app_store_id {appid}")
        else:
            print(f"No match for {appid} (trying alternate pattern)")
                                                 
            pattern2 = rf"('app_store_id' => '{re.escape(appid)}',)(\s*)('min_ios' =>)"
            replacement2 = rf"\1\n                    'icon_url' => '{new_icon}',\2\3"
            new_content, count2 = re.subn(pattern2, replacement2, content, flags=re.DOTALL)
            if count2 > 0:
                content = new_content
                updated += count2
                print(f"  Inserted icon for {appid}")

    return content, updated

if __name__ == '__main__':
    with open('backend/signum/common.php', 'r', encoding='utf-8') as f:
        content = f.read()

    new_content, n = update_icons(content)

    if n > 0:
                
        import shutil, datetime
        bak = f'backend/signum/common.php.bak.icons.{datetime.datetime.now().strftime("%Y%m%d%H%M%S")}'
        shutil.copy('backend/signum/common.php', bak)
        print(f"Backup saved to {bak}")

        with open('backend/signum/common.php', 'w', encoding='utf-8') as f:
            f.write(new_content)
        print(f"\nSuccessfully updated {n} icon(s) in common.php")
    else:
        print("No icons were updated.")
