#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "jinja2",
#   "markdown",
# ]
# ///

from __future__ import annotations

import argparse
from pathlib import Path

from jinja2 import Environment, FileSystemLoader, select_autoescape
import markdown

ASSETS = (
    ("Linux (x86_64)", "falkordb-shell-linux-x86_64"),
    ("macOS", "falkordb-shell-macos"),
    ("Windows (x86_64)", "falkordb-shell-windows-x86_64.exe"),
)
DEFAULT_TEMPLATE = Path(__file__).with_name("templates") / "release_site.html.j2"
DEFAULT_HELP_FILE = Path(__file__).resolve().parent.parent / "help.txt"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build the GitHub Pages release site from README.md."
    )
    parser.add_argument("--readme", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--release-date", required=True)
    return parser.parse_args()


def split_title(readme_text: str) -> tuple[str, str]:
    lines = readme_text.splitlines()
    if lines and lines[0].startswith("# "):
        return lines[0][2:].strip(), "\n".join(lines[1:]).lstrip()
    return "FalkorDB Shell", readme_text


def release_assets(repo: str, tag: str) -> list[dict[str, str]]:
    base_url = f"https://github.com/{repo}/releases/download/{tag}"
    return [
        {"label": label, "url": f"{base_url}/{asset}"}
        for label, asset in ASSETS
    ]


def render_page(
    *,
    template_path: Path,
    title: str,
    body_html: str,
    version: str,
    release_date: str,
    downloads: list[dict[str, str]],
    help_text: str,
) -> str:
    environment = Environment(
        loader=FileSystemLoader(template_path.parent),
        autoescape=select_autoescape(["html", "xml"]),
    )
    template = environment.get_template(template_path.name)
    return template.render(
        title=title,
        body_html=body_html,
        version=version,
        release_date=release_date,
        downloads=downloads,
        help_text=help_text,
    )


def main() -> None:
    args = parse_args()
    title, readme_body = split_title(args.readme.read_text(encoding="utf-8"))
    help_text = DEFAULT_HELP_FILE.read_text(encoding="utf-8").strip()
    body_html = markdown.markdown(readme_body, extensions=["fenced_code", "tables"])
    page = render_page(
        template_path=DEFAULT_TEMPLATE,
        title=title,
        body_html=body_html,
        version=args.version,
        release_date=args.release_date,
        downloads=release_assets(args.repo, args.tag),
        help_text=help_text,
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(page, encoding="utf-8")


if __name__ == "__main__":
    main()
