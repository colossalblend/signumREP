                      
import re
from pathlib import Path

path = Path(__file__).resolve().parent / "signum" / "common.php"
text = path.read_text(encoding="utf-8")

                                              
text2, n1 = re.subn(
    r"\n            // Test-only: hide from everyone except desktop client 1\.0\.3\n"
    r"            'visible_for_versions' => \['1\.0\.3'\],",
    "",
    text,
)

                            
text2, n2 = re.subn(
    r"\n            'visible_for_versions' => \['1\.0\.3'\],",
    "",
    text2,
)

text2 = text2.replace(
    "const SIGNUM_LATEST_VERSION = '1.0.2';",
    "const SIGNUM_LATEST_VERSION = '1.0.3';",
)
text2 = text2.replace(
    "const SIGNUM_LATEST_BUILD = 5;",
    "const SIGNUM_LATEST_BUILD = 6;",
)

path.write_text(text2, encoding="utf-8")

        
print(f"removed with comment: {n1}")
print(f"removed bare: {n2}")
print("visible_for_versions app entries left:", text2.count("'visible_for_versions' =>"))
for line in text2.splitlines():
    if "SIGNUM_LATEST" in line or "SIGNUM_MIN" in line:
        print(line)
