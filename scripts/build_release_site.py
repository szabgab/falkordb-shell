#!/usr/bin/env python3

from __future__ import annotations

import argparse
import html
from pathlib import Path

import markdown

ASSETS = (
    ("Linux (x86_64)", "falkordb-shell-linux-x86_64"),
    ("macOS", "falkordb-shell-macos"),
    ("Windows (x86_64)", "falkordb-shell-windows-x86_64.exe"),
)


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


def render_downloads(repo: str, tag: str) -> str:
    base_url = f"https://github.com/{repo}/releases/download/{tag}"
    items = "\n".join(
        f'          <li><a href="{html.escape(f"{base_url}/{asset}")}">{html.escape(label)}</a></li>'
        for label, asset in ASSETS
    )
    return f"<ul>\n{items}\n        </ul>"


def build_page(
    *,
    title: str,
    body_html: str,
    version: str,
    release_date: str,
    downloads_html: str,
) -> str:
    return f"""<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>{html.escape(title)} {html.escape(version)}</title>
    <style>
      :root {{
        color-scheme: light dark;
        font-family: system-ui, sans-serif;
        line-height: 1.5;
      }}
      body {{
        margin: 0;
        background: #0f172a;
        color: #e2e8f0;
      }}
      main {{
        max-width: 52rem;
        margin: 0 auto;
        padding: 2rem 1.25rem 4rem;
      }}
      a {{
        color: #7dd3fc;
      }}
      .hero, .card {{
        background: #111827;
        border: 1px solid #334155;
        border-radius: 12px;
        padding: 1.5rem;
        margin-bottom: 1.5rem;
      }}
      .meta {{
        color: #cbd5e1;
      }}
      code, pre {{
        background: #020617;
        border-radius: 8px;
      }}
      code {{
        padding: 0.15rem 0.35rem;
      }}
      pre {{
        padding: 1rem;
        overflow-x: auto;
      }}
    </style>
  </head>
  <body>
    <main>
      <section class="hero">
        <h1>{html.escape(title)}</h1>
        <p class="meta">Version <strong>{html.escape(version)}</strong> released on <strong>{html.escape(release_date)}</strong>.</p>
      </section>
      <section class="card">
        <h2>Downloads</h2>
        {downloads_html}
      </section>
      <section class="card">
        {body_html}
      </section>
    </main>
  </body>
</html>
"""


def main() -> None:
    args = parse_args()
    title, readme_body = split_title(args.readme.read_text(encoding="utf-8"))
    body_html = markdown.markdown(readme_body, extensions=["fenced_code", "tables"])
    downloads_html = render_downloads(args.repo, args.tag)
    page = build_page(
        title=title,
        body_html=body_html,
        version=args.version,
        release_date=args.release_date,
        downloads_html=downloads_html,
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(page, encoding="utf-8")


if __name__ == "__main__":
    main()
