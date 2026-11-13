"""Keep the app and the preview page on the same rig.

The cut layers are made here (design/pet-art/<pet>/rig) and shipped from
app/public/rig/<pet>. The runtime lives in app/src/rig and the preview page
keeps a copy of it.

    python design/pet-art/tools/sync_app.py
"""
import shutil
from pathlib import Path

ART = Path(__file__).resolve().parents[1]
APP = ART.parents[1] / "app"
PETS = ["zimeng", "mashu", "paopao", "gugu", "rongrong"]
RUNTIME = ["rig.js", "perform.js"]


def main():
    for name in RUNTIME:
        shutil.copyfile(APP / "src" / "rig" / name, ART / "preview" / name)
        print(f"app/src/rig/{name} -> preview/{name}")
    for pet in PETS:
        src, dst = ART / pet / "rig", APP / "public" / "rig" / pet
        if dst.exists():
            shutil.rmtree(dst)
        shutil.copytree(src, dst)
        print(f"{pet}/rig -> app/public/rig/{pet} ({sum(1 for _ in dst.iterdir())} files)")


if __name__ == "__main__":
    main()
