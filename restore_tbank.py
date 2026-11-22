import re

                
with open('backend/signum/common.php', 'r', encoding='utf-8') as f:
    main_content = f.read()

                
with open('downloads/common.php', 'r', encoding='utf-8') as f:
    downloads = f.read()

                                             
tbank_pattern = r"('tbank' => \[.*?'versions' => \[.*?\],\s*\n\s+\],\s*\n)"
tbank_match = re.search(tbank_pattern, downloads, re.DOTALL)
if not tbank_match:
    print("Could not find tbank in downloads")
    exit(1)

full_tbank = tbank_match.group(1)

                                  
def clean_title(title):
    return re.sub(r'\s*\([^)]*\)', '', title).strip()

full_tbank = re.sub(r"'title' => '([^']+)'", lambda m: f"'title' => '{clean_title(m.group(1))}'", full_tbank)

                        
full_tbank = full_tbank.replace('/0x500h.png', '/0x300h.png')

                                        
                                                 
                                                                  

                                                                                                                               

                                     
catalog_match = re.search(r"\$catalog = \[(.*?)\];\s*\n\s*return \$catalog;", downloads, re.DOTALL)
if not catalog_match:
    print("No catalog in downloads")
    exit(1)

full_catalog = catalog_match.group(0)

                                                             
full_catalog = re.sub(
    r"('tbank' => \[.*?'versions' => \[.*?\],\s*\n\s+\],\s*\n)",
    full_tbank,
    full_catalog,
    flags=re.DOTALL
)

                                             
main_catalog_pattern = r"(\$catalog = \[)(.*?)(\];\s*\n\s*return \$catalog;)"
fixed_main = re.sub(main_catalog_pattern, lambda m: full_catalog, main_content, flags=re.DOTALL)

with open('backend/signum/common.php', 'w', encoding='utf-8') as f:
    f.write(fixed_main)

print("Restored tbank with all covers")

        
with open('backend/signum/common.php', 'r', encoding='utf-8') as f:
    c = f.read()
titles = re.findall(r"'title' => '([^']+)'", re.search(r"'tbank' => .*?'versions' => \[(.*?)\]", c, re.DOTALL).group(1))
print("tbank versions count:", len(titles))
print("First 5:", titles[:5])
print("Last 5:", titles[-5:])
