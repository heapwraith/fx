#!/usr/bin/env python3
"""Builds the Pathdash site (GitHub Pages) into an output folder.

    python3 tools/build-site.py _site

Copies `site/`, the icon (logo and favicon) and turns `docs/GUIA.md` into
`guia/index.html`. Only the standard library: the Markdown the guide uses
(headings, paragraphs, lists, tables, code, bold, italic, links) is converted
here, and anything else fails loudly instead of being shown wrong.
"""

import html
import re
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def inline(text: str) -> str:
    """Inline Markdown: `code`, **bold**, *italic*, [links](url)."""
    out = []
    # Split out code spans first: nothing inside them is formatted.
    for k, part in enumerate(re.split(r"(`[^`]*`)", text)):
        if k % 2:
            out.append("<code>" + html.escape(part[1:-1]) + "</code>")
            continue
        s = html.escape(part, quote=False)
        s = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", s)
        s = re.sub(r"(?<![\w*])\*(?!\s)(.+?)(?<!\s)\*(?![\w*])", r"<em>\1</em>", s)

        def link(m):
            url = m.group(2)
            # Links between docs point at the repository.
            if not re.match(r"https?://|#", url):
                url = "https://github.com/heapwraith/fx/blob/main/docs/" + url
            return '<a href="%s">%s</a>' % (html.escape(url), m.group(1))

        s = re.sub(r"\[([^\]]+)\]\(([^)\s]+)\)", link, s)
        out.append(s)
    return "".join(out)


def cells(line: str) -> list:
    line = line.strip()
    if line.startswith("|"):
        line = line[1:]
    if line.endswith("|"):
        line = line[:-1]
    return [c.strip() for c in line.split("|")]


def markdown(text: str) -> str:
    lines = text.splitlines()
    out = []
    i = 0
    para = []

    def flush():
        if para:
            out.append("<p>" + inline(" ".join(para)) + "</p>")
            para.clear()

    while i < len(lines):
        line = lines[i]
        stripped = line.strip()
        if not stripped:
            flush()
            i += 1
        elif stripped.startswith("```"):
            flush()
            i += 1
            code = []
            while i < len(lines) and not lines[i].strip().startswith("```"):
                code.append(lines[i])
                i += 1
            i += 1
            out.append("<pre><code>" + html.escape("\n".join(code)) + "</code></pre>")
        elif m := re.match(r"(#{1,4})\s+(.*)", stripped):
            flush()
            level = len(m.group(1))
            title = m.group(2)
            slug = re.sub(r"[^\w]+", "-", title.lower()).strip("-")
            out.append('<h%d id="%s">%s</h%d>' % (level, slug, inline(title), level))
            i += 1
        elif stripped.startswith("|"):
            flush()
            rows = []
            while i < len(lines) and lines[i].strip().startswith("|"):
                rows.append(lines[i])
                i += 1
            head, sep, body = rows[0], rows[1] if len(rows) > 1 else "", rows[2:]
            if not re.fullmatch(r"\|?(\s*:?-+:?\s*\|)+\s*:?-*:?\s*\|?", sep.strip()):
                raise SystemExit("table without a separator line: " + head)
            t = ["<table><thead><tr>"]
            t += ["<th>%s</th>" % inline(c) for c in cells(head)]
            t.append("</tr></thead><tbody>")
            for r in body:
                t.append("<tr>" + "".join("<td>%s</td>" % inline(c) for c in cells(r)) + "</tr>")
            t.append("</tbody></table>")
            out.append("".join(t))
        elif re.match(r"([-*]|\d+\.)\s", stripped):
            flush()
            ordered = bool(re.match(r"\d+\.", stripped))
            items = []
            while i < len(lines) and lines[i].strip():
                s = lines[i].strip()
                m = re.match(r"([-*]|\d+\.)\s+(.*)", s)
                if m:
                    items.append(m.group(2))
                elif items:  # continuation line of the item above
                    items[-1] += " " + s
                i += 1
            tag = "ol" if ordered else "ul"
            out.append("<%s>%s</%s>" % (tag, "".join("<li>%s</li>" % inline(x) for x in items), tag))
        elif stripped.startswith(">") or stripped.startswith("<") or stripped.startswith("!["):
            raise SystemExit("Markdown this script does not convert: " + stripped[:60])
        else:
            para.append(stripped)
            i += 1
    flush()
    return "\n".join(out)


PAGE = """<!doctype html>
<html lang="pt-BR">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Guia de uso — Pathdash</title>
<meta name="description" content="Como usar o Pathdash: navegar, buscar, arquivos, Quick Look, abas, painel duplo, terminal, etiquetas e gerenciador padrão.">
<link rel="icon" href="../favicon.ico">
<link rel="stylesheet" href="../style.css">
</head>
<body>
<header class="wrap top">
  <a class="brand" href="../"><img src="../logo.png" alt="">Pathdash</a>
  <nav class="nav">
    <a href="../">Início</a>
    <a href="https://github.com/heapwraith/fx">GitHub</a>
    <a data-download href="https://github.com/heapwraith/fx/releases">Baixar</a>
  </nav>
</header>
<main class="wrap doc">
%s
</main>
<footer>
  <div class="wrap">
    <span>Pathdash · open source, MIT ou Apache-2.0</span>
    <span><a href="https://github.com/heapwraith/fx/blob/main/docs/GUIA.md">Este guia no GitHub</a></span>
  </div>
</footer>
<script src="../download.js"></script>
</body>
</html>
"""


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: build-site.py <output folder>")
    out = Path(sys.argv[1])
    if out.exists():
        shutil.rmtree(out)
    shutil.copytree(ROOT / "site", out)
    resources = ROOT / "crates" / "fx-ui" / "resources"
    shutil.copy(resources / "logo-128.png", out / "logo.png")
    shutil.copy(resources / "fx.ico", out / "favicon.ico")
    guide = markdown((ROOT / "docs" / "GUIA.md").read_text(encoding="utf-8"))
    (out / "guia").mkdir()
    (out / "guia" / "index.html").write_text(PAGE % guide, encoding="utf-8")
    # GitHub Pages: serve the folder as is (no Jekyll).
    (out / ".nojekyll").write_text("")
    print("site in", out)


if __name__ == "__main__":
    main()
