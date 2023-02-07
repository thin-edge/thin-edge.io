"""append workspath path to virtual environment"""
import sysconfig
from pathlib import Path


def main():
    """Main"""
    venv_path = sysconfig.get_paths()["purelib"]

    workspace = Path(venv_path) / "workspace.pth"

    test_root = Path(__file__).parent.parent
    paths = [
        str(test_root / "libraries"),
    ]

    if paths:
        print("\nAdding workspace folder to the python path\n")
        for path in paths:
            print(f"Adding path to .venv paths: {path}")
        print("\n")

    workspace.write_text("\n".join(paths))


if __name__ == "__main__":
    main()
