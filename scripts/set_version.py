#!/usr/bin/env python3
from __future__ import annotations

import re
from pathlib import Path

import click


ROOT_MARKERS = ("Cargo.toml", "package.json")


def find_repo_root(start: Path) -> Path:
    current = start
    while current != current.parent:
        if all((current / marker).exists() for marker in ROOT_MARKERS):
            return current
        current = current.parent
    raise FileNotFoundError("Could not locate repo root from script location.")


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


def update_cargo_version(cargo_toml: Path, version: str) -> bool:
    text, newline = read_text_with_eol(cargo_toml)
    pattern = re.compile(r'(?m)^(version\s*=\s*")(.+?)(")')
    updated, count = pattern.subn(lambda m: f"{m.group(1)}{version}{m.group(3)}", text, count=1)
    if count == 0:
        raise ValueError(f"Could not find version in {cargo_toml}")
    if updated == text:
        return False
    write_text_with_eol(cargo_toml, updated, newline)
    return True


def update_gui_version(package_json: Path, version: str) -> bool:
    text, newline = read_text_with_eol(package_json)
    pattern = re.compile(r'(?m)^(\s*"version"\s*:\s*")(.+?)(")')
    updated, count = pattern.subn(lambda m: f"{m.group(1)}{version}{m.group(3)}", text, count=1)
    if count == 0:
        raise ValueError(f"Could not find version in {package_json}")
    if updated == text:
        return False
    write_text_with_eol(package_json, updated, newline)
    return True


def resolve_project_paths(repo_root: Path, project: str) -> tuple[Path, Path]:
    project_dir = repo_root / "custom_plugins" / project
    cargo_toml = project_dir / "Cargo.toml"
    gui_package = project_dir / "web-gui" / "package.json"

    if not project_dir.exists():
        raise FileNotFoundError(f"Project not found: {project_dir}")
    if not cargo_toml.exists():
        raise FileNotFoundError(f"Missing Cargo.toml: {cargo_toml}")
    if not gui_package.exists():
        raise FileNotFoundError(f"Missing web gui package.json: {gui_package}")

    return cargo_toml, gui_package


@click.command()
@click.argument("project")
@click.argument("version")
@click.option(
    "--plugin-only",
    is_flag=True,
    help="Only set the plugin (Cargo.toml) version.",
)
@click.option(
    "--gui-only",
    is_flag=True,
    help="Only set the gui (web-gui/package.json) version.",
)
def main(project: str, version: str, plugin_only: bool, gui_only: bool) -> None:
    if plugin_only and gui_only:
        raise click.UsageError("Choose either --plugin-only or --gui-only, not both.")

    repo_root = find_repo_root(Path(__file__).resolve())
    cargo_toml, gui_package = resolve_project_paths(repo_root, project)

    try:
        if not plugin_only and not gui_only:
            updated_plugin = update_cargo_version(cargo_toml, version)
            updated_gui = update_gui_version(gui_package, version)
        elif plugin_only:
            updated_plugin = update_cargo_version(cargo_toml, version)
            updated_gui = False
        else:
            updated_plugin = False
            updated_gui = update_gui_version(gui_package, version)
    except (FileNotFoundError, ValueError) as exc:
        click.echo(f"Error: {exc}", err=True)
        raise SystemExit(1)

    if not updated_plugin and not updated_gui:
        click.echo("No changes made (version already set or not found).")
        return

    if updated_plugin:
        click.echo(f"Updated plugin version in {cargo_toml}")
    if updated_gui:
        click.echo(f"Updated gui version in {gui_package}")


if __name__ == "__main__":
    main()
