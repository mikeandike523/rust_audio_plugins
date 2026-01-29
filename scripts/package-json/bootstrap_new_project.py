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


def read_text_with_eol(path: Path) -> tuple[str, str]:
    data = path.read_bytes()
    text = data.decode("utf-8")

    crlf = data.count(b"\r\n")
    lf = data.count(b"\n") - crlf
    cr = data.count(b"\r") - crlf

    if crlf >= lf and crlf >= cr and crlf > 0:
        newline = "\r\n"
    elif cr > 0 and cr >= lf:
        newline = "\r"
    else:
        newline = "\n"

    normalized = text.replace("\r\n", "\n").replace("\r", "\n")
    return normalized, newline


def write_text_with_eol(path: Path, text: str, newline: str) -> None:
    payload = text if newline == "\n" else text.replace("\n", newline)
    with path.open("w", encoding="utf-8", newline="") as handle:
        handle.write(payload)


def to_pascal_case(name: str) -> str:
    return "".join(part.capitalize() for part in name.split("_") if part)


def make_vst3_class_id(template: str, new_name: str) -> str:
    if "_" in template:
        prefix = template.split("_", 1)[0] + "_"
    else:
        prefix = ""

    max_len = 16
    remaining = max_len - len(prefix)
    base = re.sub(r"[^A-Za-z0-9]", "", to_pascal_case(new_name))

    if remaining <= 0:
        return prefix[:max_len].ljust(max_len, "_")

    trimmed = base[:remaining]
    return prefix + trimmed.ljust(remaining, "_")


def make_clap_id(template: str, new_name: str) -> str:
    if "." in template:
        prefix = template.rsplit(".", 1)[0] + "."
        return prefix + new_name
    return new_name


def update_plugin_ids(lib_rs: Path, new_name: str) -> None:
    text, newline = read_text_with_eol(lib_rs)

    def replace_clap(match: re.Match[str]) -> str:
        updated = make_clap_id(match.group(2), new_name)
        return f"{match.group(1)}{updated}{match.group(3)}"

    def replace_vst(match: re.Match[str]) -> str:
        updated = make_vst3_class_id(match.group(2), new_name)
        return f"{match.group(1)}{updated}{match.group(3)}"

    updated = re.sub(
        r'(?m)^(\s*const\s+CLAP_ID\s*:\s*&\'static str\s*=\s*")(.*?)(";)',
        replace_clap,
        text,
        count=1,
    )
    updated = re.sub(
        r'(?m)^(\s*const\s+VST3_CLASS_ID\s*:\s*\[u8;\s*16\]\s*=\s*\*b")(.*?)(";)',
        replace_vst,
        updated,
        count=1,
    )

    if updated != text:
        write_text_with_eol(lib_rs, updated, newline)


def update_text_files(dest_dir: Path, new_name: str) -> None:
    new_kebab = new_name.replace("_", "-")
    new_title = " ".join(part.capitalize() for part in new_name.split("_"))

    for path in dest_dir.rglob("*"):
        if not path.is_file():
            continue
        try:
            text, newline = read_text_with_eol(path)
        except UnicodeDecodeError:
            continue

        updated = text.replace("basic_plugin_example", new_name)
        updated = updated.replace("basic-plugin-example", new_kebab)
        updated = updated.replace("Basic Plugin Example", new_title)

        if updated != text:
            write_text_with_eol(path, updated, newline)


def update_cargo_toml(cargo_toml: Path, new_name: str) -> None:
    text, newline = read_text_with_eol(cargo_toml)
    text = re.sub(r'(?m)^name\s*=\s*".*?"', f'name = "{new_name}"', text, count=1)
    text = re.sub(r'(?m)^version\s*=\s*".*?"', 'version = "0.0.0"', text, count=1)
    write_text_with_eol(cargo_toml, text, newline)


def update_web_package_json(package_json: Path, new_name: str) -> None:
    text, newline = read_text_with_eol(package_json)
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
    write_text_with_eol(package_json, text, newline)


def add_workspace_member(root_cargo: Path, new_name: str) -> None:
    text, newline = read_text_with_eol(root_cargo)
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
    write_text_with_eol(root_cargo, updated, newline)


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
    update_plugin_ids(dest_dir / "src" / "lib.rs", args.name)
    update_cargo_toml(dest_dir / "Cargo.toml", args.name)
    update_web_package_json(dest_dir / "web-gui" / "package.json", args.name)
    add_workspace_member(repo_root / "Cargo.toml", args.name)

    print(f"Bootstrapped new plugin at {dest_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
