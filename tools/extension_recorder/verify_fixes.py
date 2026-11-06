import sys
import shutil
from pathlib import Path

sys.path.append(str(Path("tools/extension_recorder").resolve()))
from recorder import SidecarProcess

sidecar_home = r"C:\cs3d\repo\cs-sidecar\build\install\cs-sidecar"
jdk21_home = r"C:\Program Files\JetBrains\IntelliJ IDEA Community Edition 2025.2.4\jbr"
data_dir = Path("tools/extension_recorder/cache/fresh_verify_all")
if data_dir.exists():
    shutil.rmtree(data_dir, ignore_errors=True)

sp = SidecarProcess(sidecar_home, jdk21_home, data_dir, worker_id=77)

plugins_to_test = [
    "AltadefinizioneProvider_1.cs3",
    "FshareProvider_35.cs3",
    "SyncStream_11.cs3",
    "Cross-Device Sync_9.cs3",
    "IPTVProvider_9.cs3",
    "WatchParty_2.cs3",
    "TorraStream_97.cs3",
    "StreamCenter_13.cs3",
    "YouTube_3.cs3",
    "Ultima_70.cs3"
]

passed = 0
failed = 0

for fname in plugins_to_test:
    path = Path("tools/extension_recorder/cache") / fname
    if not path.exists():
        prefix = fname.split("_")[0]
        matches = list(Path("tools/extension_recorder/cache").glob(f"*{prefix}*"))
        if matches:
            path = matches[0]

    if path.exists():
        try:
            res, stderr = sp.call("load_extension", {"path": str(path.resolve()), "trusted": True}, timeout=20)
            if "result" in res:
                providers = [item.get("name") for item in res["result"]]
                print(f" [PASS] {path.name:32s} -> {len(providers)} provider(s): {providers}")
                passed += 1
            else:
                err = res.get("error", {})
                print(f" [FAIL] {path.name:32s} -> {err.get('type')}: {err.get('message', '')[:70]}")
                failed += 1
        except Exception as ex:
            print(f" [ERR]  {path.name:32s} -> {ex}")
            failed += 1
    else:
        print(f" [MISS] {fname}")

print(f"\nTotal: {passed} passed, {failed} failed")
sp.stop()
