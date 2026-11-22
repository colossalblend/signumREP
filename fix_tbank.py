import re

with open('backend/signum/common.php', 'r', encoding='utf-8') as f:
    content = f.read()

                                 
toastmas_pattern = r"(\s*\[\s*'title' => 'Toastmas'.*?in_store' => false,\s*\],\s*)"

match = re.search(toastmas_pattern, content, re.DOTALL)
if not match:
    print("Toastmas block not found")
    exit(1)

toastmas_block = match.group(1)

                            
versions_start_match = re.search(r"('versions' => \[)", content)
if not versions_start_match:
    print("versions start not found")
    exit(1)

versions_start_pos = versions_start_match.start(1)

                                                                                 
                                                     

                                                     
                                                                 

                                                                                              
                                                                

                                                                   
tbank_start = content.find("'tbank' => [")
end_match = re.search(r"\],\s*\n\s+\],\s*\n\s+\]", content[tbank_start:])
if not end_match:
    print("end not found")
    exit(1)

versions_end_pos = tbank_start + end_match.end()

                           
new_versions = "'versions' => [\n" + toastmas_block + "            ],"

                                
before = content[:versions_start_pos]

                                                
after = content[versions_end_pos:]

new_content = before + new_versions + after

with open('backend/signum/common.php', 'w', encoding='utf-8') as f:
    f.write(new_content)

print("Fixed: only Toastmas remains in tbank versions")

        
with open('backend/signum/common.php', 'r', encoding='utf-8') as f:
    c = f.read()
tbank_versions = re.search(r"'tbank' => .*?'versions' => \[(.*?)\]", c, re.DOTALL)
if tbank_versions:
    titles = re.findall(r"'title' => '([^']+)'", tbank_versions.group(1))
    print("Current tbank versions:", titles)
