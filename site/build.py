#!/usr/bin/env python3
"""Assemble site/index.html from the per-piece sources in site/src/.

Each piece owns one directory under src/pieces/<name>/ containing an optional
style.css and an optional markup.html. Pieces are concatenated in the order
listed in src/order.json, so two builders never touch the same file.
"""
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent
SRC = ROOT / "src"
PIECES = SRC / "pieces"


def read(p):
    return p.read_text(encoding="utf-8") if p.exists() else ""


def main():
    order = json.loads(read(SRC / "order.json") or "[]")
    css, body, head_extra = [], [], []
    missing = []

    for name in order:
        d = PIECES / name
        if not d.is_dir():
            missing.append(name)
            continue
        style, markup = d / "style.css", d / "markup.html"
        if (d / "head.html").exists():
            head_extra.append(read(d / "head.html").rstrip())
        if style.exists():
            css.append("/* ===== %s ===== */\n%s" % (name, read(style).rstrip()))
        if markup.exists():
            body.append("<!-- ===== %s ===== -->\n%s" % (name, read(markup).rstrip()))

    head = read(SRC / "head.html").rstrip()
    tail = read(SRC / "tail.html").rstrip()

    out = "\n".join([
        "<!doctype html>",
        '<html lang="en">',
        "<head>",
        head,
        "\n".join(head_extra),
        "<style>",
        "\n\n".join(css),
        "</style>",
        "</head>",
        "<body>",
        "\n\n".join(body),
        tail,
        "</body>",
        "</html>",
        "",
    ])

    (ROOT / "index.html").write_text(out, encoding="utf-8")

    # standalone type-specimen sheet, same foundation CSS, for blind critique
    spec = read(SRC / "specimen.html")
    if spec.strip():
        (ROOT / "specimen.html").write_text("\n".join([
            "<!doctype html>", '<html lang="en">', "<head>", head,
            "\n".join(head_extra), "<style>", "\n\n".join(css), "</style>",
            "</head>", "<body>", spec, "</body>", "</html>", "",
        ]), encoding="utf-8")
    print("built index.html  %d pieces  %d bytes" % (len(order), len(out)))
    if missing:
        print("MISSING pieces (not yet built): %s" % ", ".join(missing), file=sys.stderr)


if __name__ == "__main__":
    main()
