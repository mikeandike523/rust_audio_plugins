#!/usr/bin/env python3
from __future__ import annotations

import re
import sys
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


def read_cargo_version(cargo_toml: Path) -> str:
    text, _ = read_text_with_eol(cargo_toml)
    match = re.search(r'(?m)^version\s*=\s*"(.*?)"', text)
    if not match:
        raise ValueError(f"Could not find version in {cargo_toml}")
    return match.group(1)


def read_gui_version(package_json: Path) -> str:
    text, _ = read_text_with_eol(package_json)
    match = re.search(r'(?m)^\s*"version"\s*:\s*"(.*?)"', text)
    if not match:
        raise ValueError(f"Could not find version in {package_json}")
    return match.group(1)


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
def main(project: str) -> None:
   
    try:
        repo_root = find_repo_root(Path(__file__).resolve())
        cargo_toml, gui_package = resolve_project_paths(repo_root, project)
        plugin_version = read_cargo_version(cargo_toml)
        gui_version = read_gui_version(gui_package)
    except (FileNotFoundError, ValueError) as exc:
        click.echo(f"Error: {exc}", err=True)
        raise SystemExit(1)

    click.echo(f"Plugin version: {plugin_version}")
    click.echo(f"GUI version: {gui_version}")

if __name__ == "__main__":
    main()
