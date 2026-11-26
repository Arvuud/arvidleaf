#!/usr/bin/env python3
"""Writes THIRD-PARTY-LICENSES.txt for a binary package of arvidleaf.

Lists every crate compiled into the program (normal dependencies of the `arvidleaf`
binary for the build platform; no dev or build dependencies) with its license and the
license texts shipped in the crate's sources. Identical texts are printed once with the
crates that use them. Crates without license files get the standard text of their SPDX
license with the authors from Cargo.toml. The fonts egui embeds (epaint_default_fonts)
are included with their licenses.

    packaging/third-party-licenses.py > THIRD-PARTY-LICENSES.txt

Needs cargo (crate sources are fetched as for a build).
"""
import hashlib
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LICENSE_FILE = re.compile(r"^(licen[cs]e|copying|notice|copyright|unlicense|ofl|ufl)([-_.].*)?$", re.I)
MIT_TEMPLATE = """Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
"""


def host_triple():
    out = subprocess.run(["rustc", "-vV"], capture_output=True, text=True, check=True).stdout
    return next(l.split(":", 1)[1].strip() for l in out.splitlines() if l.startswith("host:"))


def shipped_packages(meta):
    """Packages reachable from the arvidleaf binary through normal dependencies."""
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    pkgs = {p["id"]: p for p in meta["packages"]}
    root = next(p["id"] for p in meta["packages"] if p["name"] == "arvidleaf")
    seen, todo = set(), [root]
    while todo:
        pid = todo.pop()
        if pid in seen:
            continue
        seen.add(pid)
        for d in nodes[pid]["deps"]:
            if any(k["kind"] is None for k in d["dep_kinds"]):
                todo.append(d["pkg"])
    members = set(meta["workspace_members"])
    return sorted((pkgs[i] for i in seen if i not in members), key=lambda p: (p["name"], p["version"]))


def license_texts(pkg):
    """(file name, text) of the license files in the crate (and its fonts/ folder)."""
    base = os.path.dirname(pkg["manifest_path"])
    files = []
    for d in (base, os.path.join(base, "fonts")):
        if not os.path.isdir(d):
            continue
        for name in sorted(os.listdir(d)):
            path = os.path.join(d, name)
            if not os.path.isfile(path):
                continue
            fonts = os.path.basename(d) == "fonts" and name.endswith(".txt")
            if LICENSE_FILE.match(name) or fonts:
                with open(path, encoding="utf-8", errors="replace") as f:
                    files.append((os.path.relpath(path, base), f.read().strip() + "\n"))
    return files


def standard_text(pkg):
    """Fallback for crates without license files: SPDX text with the Cargo.toml authors."""
    lic = pkg.get("license") or "see the crate's repository"
    authors = ", ".join(pkg.get("authors") or []) or "the authors of " + pkg["name"]
    parts = []
    if "MIT" in lic:
        parts.append(f"MIT License\n\nCopyright (c) {authors}\n\n{MIT_TEMPLATE}")
    if "Apache-2.0" in lic:
        parts.append(f"Copyright (c) {authors}\nLicensed under the Apache License, Version 2.0 (full text at the end of this file).\n")
    if not parts:
        parts.append(f"License: {lic}\nCopyright (c) {authors}\n")
    return [("(no license file in the crate; standard text)", "\n".join(parts))]


def main():
    env = dict(os.environ)
    meta = json.loads(subprocess.run(["cargo", "metadata", "--format-version", "1", "--filter-platform", host_triple()], cwd=ROOT, capture_output=True, text=True, check=True, env=env).stdout)
    pkgs = shipped_packages(meta)
    groups = {}  # text hash -> (texts, [crates])
    for p in pkgs:
        texts = license_texts(p) or standard_text(p)
        key = hashlib.sha256("".join(t for _, t in texts).encode()).hexdigest()
        groups.setdefault(key, (texts, []))[1].append(p)
    out = sys.stdout
    out.write("Third-party software in arvidleaf\n" + "=" * 33 + "\n\n")
    out.write("arvidleaf itself is licensed under MIT OR Apache-2.0 (LICENSE-MIT, LICENSE-APACHE).\n")
    out.write(f"It contains the following {len(pkgs)} Rust crates. External programs it runs (TeX,\n")
    out.write("latexmk, biber, poppler, curl, sqlite3) are not part of it.\n\n")
    for p in pkgs:
        out.write(f"  {p['name']} {p['version']}  ({p.get('license') or 'see repository'})\n")
    out.write("\n")
    for texts, crates in sorted(groups.values(), key=lambda g: g[1][0]["name"]):
        out.write("-" * 78 + "\n")
        out.write("Used by: " + ", ".join(f"{c['name']} {c['version']}" for c in crates) + "\n\n")
        for name, text in texts:
            out.write(f"--- {name}\n\n{text}\n")
    with open(os.path.join(ROOT, "LICENSE-APACHE"), encoding="utf-8") as f:
        out.write("-" * 78 + "\nApache License, Version 2.0 (full text)\n\n" + f.read())


if __name__ == "__main__":
    main()
