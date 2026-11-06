import json
from collections import Counter
from pathlib import Path

results_file = Path("tools/extension_recorder/replies/results.json")
if not results_file.exists():
    print("results.json not found")
    exit(1)

results = json.loads(results_file.read_text(encoding="utf-8"))
failed = [r for r in results if r.get("status") == "error"]

print(f"Total tested : {len(results)}")
print(f"Total passed : {len([r for r in results if r.get('status') == 'success'])}")
print(f"Total failed : {len(failed)}")

by_type = {}
for r in failed:
    err = r.get("error") or {}
    t = err.get("type", "Unknown")
    by_type.setdefault(t, []).append(r)

for err_type, items in sorted(by_type.items(), key=lambda x: len(x[1]), reverse=True):
    print(f"\n=======================================================")
    print(f"ERROR TYPE: {err_type} ({len(items)} extensions)")
    print(f"=======================================================")
    repo_counts = Counter(x["repository_name"] for x in items)
    print("Repositories affected:", dict(repo_counts.most_common(5)))
    print("Sample error messages:")
    for item in items[:4]:
        msg = (item.get("error") or {}).get("message", "")
        print(f"  * [{item['plugin_name']}] (v{item['version']}): {msg}")
