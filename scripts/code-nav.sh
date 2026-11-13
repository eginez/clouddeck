#!/usr/bin/env bash
# code-nav: find and read Rust code by symbol instead of grep + whole-file reads.
#
#   scripts/code-nav.sh outline <file.rs>     items in a file with line ranges
#   scripts/code-nav.sh def <symbol>          file:line of each definition
#   scripts/code-nav.sh show <symbol>         only that item's source (with line numbers)
#   scripts/code-nav.sh refs <symbol>         file:line of each mention of the bare name (text match)
#   scripts/code-nav.sh refs <symbol> --precise   semantic uses via SCIP index
#
# <symbol> is a bare name (aws_files_for) or Type::name (AwsFiles::new).
# outline/def/show are syntax-only (rust-analyzer `symbols`, per-file cache): always
# fresh, about a second. refs --precise uses `rust-analyzer scip`, cached in target/code-nav
# and rebuilt when any .rs file is newer than the index (~30 s warm, ~5 GB peak RSS).
set -euo pipefail
rust-analyzer --version >/dev/null 2>&1 || { echo "code-nav: rust-analyzer not usable (rustup component add rust-analyzer)" >&2; exit 2; }
exec python3 - "$@" <<'PY'
import os, re, signal, subprocess, sys
signal.signal(signal.SIGPIPE, signal.SIG_DFL)
from concurrent.futures import ThreadPoolExecutor

ROOT = subprocess.run(["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True).stdout.strip() or os.getcwd()
CACHE = os.path.join(ROOT, "target", "code-nav")
SHOW_MAX = 6  # print at most this many items for `show`; list the rest
REFS_TEXT_MAX = 60

NODE = re.compile(r'StructureNode \{ parent: (None|Some\((\d+)\)), label: "((?:[^"\\]|\\.)*)", '
                  r'navigation_range: (\d+)\.\.(\d+), node_range: (\d+)\.\.(\d+), kind: (\w+)(?:\((\w+)\))?, '
                  r'detail: (None|Some\("((?:[^"\\]|\\.)*)"\))')
KIND = {"Function": "fn", "Method": "fn", "Struct": "struct", "Enum": "enum", "Trait": "trait",
        "Impl": "impl", "Module": "mod", "Const": "const", "Static": "static", "TypeAlias": "type",
        "Macro": "macro", "Union": "union", "Field": "field", "Variant": "variant"}
SKIP_IN_OUTLINE = {"Field", "Variant", "Local", "TypeParam", "LifetimeParam", "ConstParam"}

def rs_files():
    out = subprocess.run(["git", "ls-files", "--cached", "--others", "--exclude-standard", "*.rs"],
                         cwd=ROOT, capture_output=True, text=True).stdout.split()
    return [f for f in out if os.path.exists(os.path.join(ROOT, f))]

def line_starts(data):
    starts = [0]
    for i, b in enumerate(data):
        if b == 10: starts.append(i + 1)
    return starts

def to_line(starts, off):
    lo, hi = 0, len(starts) - 1
    while lo < hi:
        mid = (lo + hi + 1) // 2
        if starts[mid] <= off: lo = mid
        else: hi = mid - 1
    return lo + 1

def symbols(rel):
    """[(idx, parent, label, kind, detail, nav_line, start_line, end_line)] for one file, cached."""
    src = os.path.join(ROOT, rel)
    cache = os.path.join(CACHE, "sym", rel + ".tsv")
    if os.path.exists(cache) and os.path.getmtime(cache) >= os.path.getmtime(src):
        rows = []
        for ln in open(cache, encoding="utf-8"):
            p = ln.rstrip("\n").split("\t")
            rows.append((int(p[0]), None if p[1] == "" else int(p[1]), p[2], p[3], p[4], int(p[5]), int(p[6]), int(p[7])))
        return rows
    data = open(src, "rb").read()
    r = subprocess.run(["rust-analyzer", "symbols"], input=data, capture_output=True)
    if r.returncode != 0:
        sys.stderr.write(f"code-nav: rust-analyzer symbols failed on {rel}\n{r.stderr.decode(errors='replace')[-2000:]}")
        sys.exit(1)
    out = r.stdout.decode("utf-8", "replace")
    starts = line_starts(data)
    rows = []
    for i, m in enumerate(NODE.finditer(out)):
        parent = int(m.group(2)) if m.group(2) else None
        kind = m.group(9) or m.group(8)
        detail = (m.group(11) or "").replace('\\"', '"')
        rows.append((i, parent, m.group(3).replace('\\"', '"'), kind, detail,
                     to_line(starts, int(m.group(4))), to_line(starts, int(m.group(6))),
                     to_line(starts, max(int(m.group(7)) - 1, int(m.group(6))))))
    os.makedirs(os.path.dirname(cache), exist_ok=True)
    with open(cache, "w", encoding="utf-8") as f:
        for r in rows:
            f.write("\t".join("" if v is None else str(v).replace("\t", " ").replace("\n", " ") for v in r) + "\n")
    return rows

def impl_type(label):
    """'impl<T> Foo for Bar<T>' -> 'Bar'; 'impl Bar' -> 'Bar'."""
    s = label[4:].strip() if label.startswith("impl") else label
    if " for " in s: s = s.split(" for ", 1)[1]
    s = re.sub(r"^<[^>]*>\s*", "", s).strip().lstrip("&").replace("mut ", "")
    return re.split(r"[<\s]", s.split("::")[-1], 1)[0]

def all_defs():
    files = rs_files()
    with ThreadPoolExecutor(max_workers=4) as ex:
        res = list(ex.map(symbols, files))
    for rel, rows in zip(files, res):
        by_idx = {r[0]: r for r in rows}
        for r in rows:
            owner = None
            if r[1] is not None:
                p = by_idx.get(r[1])
                if p: owner = impl_type(p[2]) if p[3] == "Impl" else p[2]
            yield rel, owner, r

def find(query):
    parts = query.split("::")
    name, owner = parts[-1], (parts[-2] if len(parts) > 1 else None)
    hits = []
    for rel, own, r in all_defs():
        if r[3] in ("Impl", "Local"): continue
        if r[2] != name: continue
        if owner and own != owner and not rel.replace("/", "::").removesuffix(".rs").endswith(owner): continue
        hits.append((rel, own, r))
    # definitions before fields/variants with the same name
    hits.sort(key=lambda h: (h[2][3] in ("Field", "Variant"), h[0], h[2][5]))
    return hits

def desc(own, r):
    k = KIND.get(r[3], r[3].lower())
    name = f"{own}::{r[2]}" if own else r[2]
    return f"{k} {name}" + (f"  {r[4]}" if r[4] else "")

def cmd_outline(path):
    rel = os.path.relpath(os.path.abspath(path), ROOT)
    rows = symbols(rel)
    by_idx = {r[0]: r for r in rows}
    print(f"{rel} ({sum(1 for _ in open(os.path.join(ROOT, rel), 'rb'))} lines)")
    def depth(r):
        d = 0
        while r[1] is not None and r[1] in by_idx:
            r = by_idx[r[1]]; d += 1
        return d
    for r in rows:
        if r[3] in SKIP_IN_OUTLINE: continue
        d = depth(r)
        if d > 2: continue
        k = KIND.get(r[3], r[3].lower())
        label = r[2] if r[3] == "Impl" else f"{k} {r[2]}"
        detail = f"  {r[4]}" if r[4] and r[3] in ("Function", "Method") else ""
        if len(detail) > 110: detail = detail[:107] + "..."
        print(f"{r[6]:>5}-{r[7]:<5} {'  ' * d}{label}{detail}")

def cmd_def(query):
    hits = find(query)
    if not hits: print(f"no definition named {query}", file=sys.stderr); return 1
    for rel, own, r in hits:
        print(f"{rel}:{r[5]}  (lines {r[6]}-{r[7]})  {desc(own, r)}")

def cmd_show(query):
    hits = find(query)
    if not hits: print(f"no definition named {query}", file=sys.stderr); return 1
    for rel, own, r in hits[:SHOW_MAX]:
        lines = open(os.path.join(ROOT, rel), encoding="utf-8", errors="replace").read().split("\n")
        print(f"==> {rel}:{r[6]}-{r[7]}  {desc(own, r)}")
        for n in range(r[6], r[7] + 1):
            print(f"{n:>5}  {lines[n - 1]}")
    if len(hits) > SHOW_MAX:
        print(f"... {len(hits) - SHOW_MAX} more; narrow with Type::name or use def:")
        for rel, own, r in hits[SHOW_MAX:]:
            print(f"    {rel}:{r[5]}  {desc(own, r)}")

# ---- refs: SCIP index -------------------------------------------------------------
def varint(b, i):
    v = s = 0
    while True:
        c = b[i]; i += 1; v |= (c & 0x7F) << s; s += 7
        if c < 0x80: return v, i

def fields(b):
    i, n = 0, len(b)
    while i < n:
        key, i = varint(b, i); f, wt = key >> 3, key & 7
        if wt == 0: v, i = varint(b, i)
        elif wt == 2:
            ln, i = varint(b, i); v = b[i:i + ln]; i += ln
        elif wt == 1: v = b[i:i + 8]; i += 8
        elif wt == 5: v = b[i:i + 4]; i += 4
        else: raise ValueError("bad wire type")
        yield f, wt, v

def packed(v):
    out, i = [], 0
    while i < len(v):
        x, i = varint(v, i); out.append(x)
    return out

DESC = re.compile(r"(`(?:[^`]|``)*`|[A-Za-z0-9_$+\-]+)(\([^)]*\)\.|[/#.!:])|\[[^\]]*\]|\([^)]*\)")

def sym_names(sym):
    """'... mod/impl#[Type][Trait]method().' -> ['mod','Type','method']"""
    parts = sym.split(" ", 4)
    if len(parts) < 5: return []
    names, pending_impl = [], False
    for m in DESC.finditer(parts[4]):
        tok = m.group(0)
        if m.group(1):
            if m.group(1) == "impl" and m.group(2) == "#": pending_impl = True; continue
            names.append(m.group(1).strip("`")); pending_impl = False
        elif tok.startswith("[") and pending_impl:  # impl#[Type][Trait]: keep the self type
            names.append(tok[1:-1].strip("`")); pending_impl = False
    return names

def build_index():
    idx = os.path.join(CACHE, "index.scip")
    tsv = os.path.join(CACHE, "refs.tsv")
    newest = max((os.path.getmtime(os.path.join(ROOT, f)) for f in rs_files()), default=0)
    if os.path.exists(tsv) and os.path.getmtime(tsv) >= newest: return tsv
    os.makedirs(CACHE, exist_ok=True)
    if not (os.path.exists(idx) and os.path.getmtime(idx) >= newest):
        print("code-nav: building SCIP index (about 30 s, longer on a cold target dir)...", file=sys.stderr)
        r = subprocess.run(["rust-analyzer", "scip", ".", "--output", idx, "--num-threads", "3"],
                           cwd=ROOT, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        if r.returncode != 0:
            sys.stderr.write(r.stderr.decode()[-2000:]); sys.exit(1)
    data = open(idx, "rb").read()
    with open(tsv + ".tmp", "w", encoding="utf-8") as out:
        for f, _, doc in fields(data):
            if f != 2: continue
            path, occs = "", []
            for g, _, v in fields(doc):
                if g == 1: path = v.decode()
                elif g == 2: occs.append(v)
            for occ in occs:
                rng, sym, roles = [], "", 0
                for h, wt, v in fields(occ):
                    if h == 1: rng = packed(v) if wt == 2 else rng + [v]
                    elif h == 2: sym = v.decode()
                    elif h == 3: roles = v
                if not sym or sym.startswith("local ") or not rng: continue
                names = sym_names(sym)
                if not names: continue
                out.write(f"{names[-1]}\t{'::'.join(names)}\t{path}\t{rng[0] + 1}\t{roles & 1}\n")
    os.replace(tsv + ".tmp", tsv)
    return tsv

def cmd_refs_text(query):
    name = query.split("::")[-1]
    pat = re.compile(r"\b" + re.escape(name) + r"\b")
    hits = []
    for rel in rs_files():
        try: lines = open(os.path.join(ROOT, rel), encoding="utf-8", errors="replace").read().split("\n")
        except OSError: continue
        hits += [(rel, n, text.strip()) for n, text in enumerate(lines, 1) if pat.search(text)]
    if not hits: print(f"no references to {query}", file=sys.stderr); return 1
    print(f"text matches for '{name}' (not semantic; add --precise for SCIP uses)")
    for rel, n, text in hits[:REFS_TEXT_MAX]:
        print(f"  {rel}:{n}  {text[:120]}")
    if len(hits) > REFS_TEXT_MAX:
        print(f"  ... {len(hits) - REFS_TEXT_MAX} more; narrow with Type::name --precise")

def cmd_refs_precise(query):
    tsv = build_index()
    parts = query.split("::")
    name = parts[-1]
    src_cache = {}
    found = []
    for ln in open(tsv, encoding="utf-8"):
        n, full, path, line, is_def = ln.rstrip("\n").split("\t")
        if n != name: continue
        if len(parts) > 1 and not ("::" + full).endswith("::" + query): continue
        found.append((full, path, int(line), is_def == "1"))
    if not found: print(f"no references to {query}", file=sys.stderr); return 1
    syms = sorted({f[0] for f in found})
    for s in syms:
        rows = sorted({(p, l, d) for full, p, l, d in found if full == s})
        defs = [r for r in rows if r[2]]
        uses = [r for r in rows if not r[2]]
        print(f"{s}: {len(uses)} uses" + (f", defined at {defs[0][0]}:{defs[0][1]}" if defs else " (external)"))
        for p, l, _ in uses:
            if p not in src_cache:
                try: src_cache[p] = open(os.path.join(ROOT, p), encoding="utf-8", errors="replace").read().split("\n")
                except OSError: src_cache[p] = []
            text = src_cache[p][l - 1].strip() if l <= len(src_cache[p]) else ""
            print(f"  {p}:{l}  {text[:120]}")

def main(argv):
    precise = "--precise" in argv
    argv = [a for a in argv if a != "--precise"]
    if len(argv) != 2 or argv[0] not in ("outline", "def", "refs", "show") or (precise and argv[0] != "refs"):
        print("usage: code-nav.sh outline <file> | def <sym> | show <sym> | refs <sym> [--precise]", file=sys.stderr)
        return 2
    if argv[0] == "refs":
        return (cmd_refs_precise if precise else cmd_refs_text)(argv[1]) or 0
    return {"outline": cmd_outline, "def": cmd_def, "show": cmd_show}[argv[0]](argv[1]) or 0

sys.exit(main(sys.argv[1:]))
PY
