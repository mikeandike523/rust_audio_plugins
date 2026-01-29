#!/usr/bin/env python3
from __future__ import annotations

import argparse
import re
import shutil
import sys
from pathlib import Path


ROOT_MARKERS = ("Cargo.toml", "package.json")


def find_repo_root(start: Path) -> Path:
    current = start
    while current != current.parent:
        if all((current / marker).exists() for marker in ROOT_MARKERS):
            return current
        current = current.parent
    raise FileNotFoundError("Could not locate repo root from script location.")


def validate_name(name: str) -> None:
    if not re.fullmatch(r"[a-z][a-z0-9_]*", name):
        raise ValueError(
            "name must be snake_case and start with a letter (e.g. my_plugin)"
        )


def copy_template(source_dir: Path, dest_dir: Path) -> None:
    ignore = shutil.ignore_patterns("node_modules", "dist", "target", ".DS_Store")
    shutil.copytree(source_dir, dest_dir, ignore=ignore)


def update_text_files(dest_dir: Path, new_name: str) -> None:
    new_kebab = new_name.replace("_", "-")
    new_title = " ".join(part.capitalize() for part in new_name.split("_"))

    for path in dest_dir.rglob("*"):
        if not path.is_file():
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue

        updated = text.replace("basic_plugin_example", new_name)
        updated = updated.replace("basic-plugin-example", new_kebab)
        updated = updated.replace("Basic Plugin Example", new_title)

        if updated != text:
            path.write_text(updated, encoding="utf-8")


def update_cargo_toml(cargo_toml: Path, new_name: str) -> None:
    text = cargo_toml.read_text(encoding="utf-8")
    text = re.sub(r'(?m)^name\s*=\s*".*?"', f'name = "{new_name}"', text, count=1)
    text = re.sub(r'(?m)^version\s*=\s*".*?"', 'version = "0.0.0"', text, count=1)
    cargo_toml.write_text(text, encoding="utf-8")


def update_web_package_json(package_json: Path, new_name: str) -> None:
    text = package_json.read_text(encoding="utf-8")
    text = re.sub(
        r'(?m)^(\s*"name"\s*:\s*)".*?"',
        rf'\1"{new_name}-web-gui"',
        text,
        count=1,
    )
    text = re.sub(
        r'(?m)^(\s*"version"\s*:\s*)".*?"',
        r'\1"0.0.0"',
        text,
        count=1,
    )
    package_json.write_text(text, encoding="utf-8")


def add_workspace_member(root_cargo: Path, new_name: str) -> None:
    text = root_cargo.read_text(encoding="utf-8")
    member_entry = f'"custom_plugins/{new_name}"'

    if member_entry in text:
        return

    match = re.search(
        r'(?s)(\[workspace\].*?members\s*=\s*\[)(.*?)(\n\])', text
    )
    if not match:
        raise ValueError("Could not find workspace members list in root Cargo.toml")

    members_block = match.group(2)
    lines = members_block.splitlines()

    trailing = []
    while lines and not lines[-1].strip():
        trailing.append(lines.pop())
    trailing.reverse()

    lines.append(f'  "custom_plugins/{new_name}",')
    new_block = "\n".join(lines + trailing)

    updated = text[: match.start(2)] + new_block + text[match.end(2) :]
    root_cargo.write_text(updated, encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Bootstrap a new plugin from custom_plugins/basic_plugin_example"
    )
    parser.add_argument("name", help="New plugin name in snake_case")
    args = parser.parse_args()

    try:
        validate_name(args.name)
    except ValueError as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    repo_root = find_repo_root(Path(__file__).resolve())
    source_dir = repo_root / "custom_plugins" / "basic_plugin_example"
    dest_dir = repo_root / "custom_plugins" / args.name

    if not source_dir.exists():
        print(f"Error: template not found at {source_dir}", file=sys.stderr)
        return 1
    if dest_dir.exists():
        print(f"Error: destination already exists at {dest_dir}", file=sys.stderr)
        return 1

    copy_template(source_dir, dest_dir)
    update_text_files(dest_dir, args.name)
    update_cargo_toml(dest_dir / "Cargo.toml", args.name)
    update_web_package_json(dest_dir / "web-gui" / "package.json", args.name)
    add_workspace_member(repo_root / "Cargo.toml", args.name)

    print(f"Bootstrapped new plugin at {dest_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
