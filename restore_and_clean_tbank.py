import re

with open('downloads/common.php', 'r', encoding='utf-8') as f:
    downloads_content = f.read()

                                        
tbank_match = re.search(r"('tbank' => \[.*?'versions' => \[.*?\],\s*\n\s+\],\s*\n)", downloads_content, re.DOTALL)
if not tbank_match:
    print("tbank not found in downloads")
    exit(1)

full_tbank = tbank_match.group(1)

                                                         
                                     
versions_in_tbank = re.search(r"('versions' => \s*\[)(.*?)(\],\s*\n\s+\],)", full_tbank, re.DOTALL)
if not versions_in_tbank:
    print("versions in tbank not found")
    exit(1)

prefix = versions_in_tbank.group(1)
old_v = versions_in_tbank.group(2)
suffix = versions_in_tbank.group(3)

                                      
first_entry = re.search(r"(\[\s*'title' => 'Toastmas'.*?in_store' => false,\s*\],)", old_v, re.DOTALL)
if first_entry:
    new_v = "\n                " + first_entry.group(1) + "\n            "
else:
    new_v = old_v

new_tbank_versions = prefix + new_v + suffix

                                                
new_full_tbank = re.sub(
    r"('versions' => \s*\[)(.*?)(\],\s*\n\s+\],)",
    lambda m: new_tbank_versions,
    full_tbank,
    flags=re.DOTALL
)

                                                     
new_full_tbank = new_full_tbank.replace('/0x500h.png', '/0x300h.png')

                                                                                                           
with open('backend/signum/common.php', 'r', encoding='utf-8') as f:
    main = f.read()

                                                                                                                                                                                                         

                                                                                                     

                                                                                                                                                    

                                                 
sber_match = re.search(r"('sber' => \[.*?\],\s*\n)", main, re.DOTALL)
if not sber_match:
    print("sber not found in main")
                                
    sber_match = re.search(r"('sber' => \[.*?\],\s*\n)", downloads_content, re.DOTALL)

sber_block = sber_match.group(1) if sber_match else ""

                                                          
                                                                                                        

                                                                            
catalog_match = re.search(r"(\$catalog = \[.*?\];\s*\n\s*return \$catalog;)", downloads_content, re.DOTALL)
if not catalog_match:
    print("full catalog not found in downloads")
    exit(1)

full_catalog = catalog_match.group(1)

                                  
fixed_catalog = re.sub(
    r"('tbank' => \[.*?'versions' => \[.*?\],\s*\n\s+\],\s*\n)",
    new_full_tbank,
    full_catalog,
    flags=re.DOTALL
)

                                 
fixed_catalog = fixed_catalog.replace('/0x500h.png', '/0x300h.png')

                                          
main_catalog_pattern = r"(\$catalog = \[)(.*?)(\];\s*\n\s*return \$catalog;)"
fixed_main = re.sub(main_catalog_pattern, lambda m: fixed_catalog, main, flags=re.DOTALL)

with open('backend/signum/common.php', 'w', encoding='utf-8') as f:
    f.write(fixed_main)

print("Restored full catalog with tbank versions reduced to only Toastmas")

        
with open('backend/signum/common.php', 'r', encoding='utf-8') as f:
    c = f.read()
match = re.search(r"'tbank' => .*?'versions' => \[(.*?)\]", c, re.DOTALL)
if match:
    titles = re.findall(r"'title' => '([^']+)'", match.group(1))
    print("tbank versions now:", titles)
else:
    print("Verification: tbank versions not found")
