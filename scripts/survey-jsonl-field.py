#!/usr/bin/env python3
"""Survey a single field across all Claude Code session JSONL files.

Reads .jsonl file paths from stdin, scans every entry of the given type,
and reports the distribution of "value shapes" the field takes — where a
shape is:
  - objects → sorted comma-separated keys, e.g. obj{stdout,stderr,interrupted}
  - arrays  → element-type multiset,        e.g. array[obj{text,type}]
  - scalars → JSON type name,               e.g. string / int / bool / null

Use this before deciding whether a raw `serde_json::Value` field can be
promoted to a typed Rust enum (or split into a "mixed typed" enum with a
catch-all variant). High-frequency shapes that cluster on a stable key set
are good typed-enum candidates; long-tail divergent shapes argue for keeping
the field as `Value` plus an accessor.

Usage:
  find ~/.claude/projects -name '*.jsonl' -type f | \
      python3 scripts/survey-jsonl-field.py <field> [entry_type] [--samples N]

Examples:
  # Distribution of `toolUseResult` shapes on user entries
  find ~/.claude/projects -name '*.jsonl' | \
      python3 scripts/survey-jsonl-field.py toolUseResult user

  # Same, with 1 full sample per shape (default --samples 0 = no samples)
  find ~/.claude/projects -name '*.jsonl' | \
      python3 scripts/survey-jsonl-field.py toolUseResult user --samples 1

  # Inspect `content` on user entries (multi-form: string vs array of blocks)
  find ~/.claude/projects -name '*.jsonl' | \
      python3 scripts/survey-jsonl-field.py content user --samples 2
"""
import sys, json, collections, argparse

def shape_sig(v, depth=0, max_depth=2):
    """Build a stable shape signature so we can bucket by structural shape."""
    if v is None: return "null"
    if isinstance(v, bool): return "bool"
    if isinstance(v, int): return "int"
    if isinstance(v, float): return "float"
    if isinstance(v, str): return "string"
    if isinstance(v, list):
        if depth >= max_depth: return "array[?]"
        if not v: return "array[empty]"
        elem_types = sorted(set(shape_sig(e, depth+1, max_depth) for e in v[:5]))
        return "array[" + ",".join(elem_types) + "]"
    if isinstance(v, dict):
        if depth >= max_depth: return "obj{?}"
        if not v: return "obj{empty}"
        keys = sorted(v.keys())
        return "obj{" + ",".join(keys) + "}"
    return "?" + type(v).__name__

def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("field", help="field name to inspect (wire spelling, e.g. toolUseResult)")
    ap.add_argument("entry_type", nargs="?", default="user", help="entry type to filter (default: user)")
    ap.add_argument("--samples", type=int, default=0, help="how many samples per shape to print (0 = none)")
    ap.add_argument("--sample-len", type=int, default=300, help="truncate each sample to this many chars (default 300)")
    ap.add_argument("--top", type=int, default=20, help="show this many top shapes (default 20)")
    args = ap.parse_args()

    shapes = collections.Counter()
    samples = collections.defaultdict(list)
    total = 0
    with_field = 0
    for line in sys.stdin:
        p = line.strip()
        if not p:
            continue
        try:
            with open(p) as f:
                for ln in f:
                    try:
                        d = json.loads(ln)
                    except json.JSONDecodeError:
                        continue
                    if d.get("type") != args.entry_type:
                        continue
                    total += 1
                    if args.field not in d:
                        continue
                    with_field += 1
                    sig = shape_sig(d[args.field], depth=0)
                    shapes[sig] += 1
                    if args.samples and len(samples[sig]) < args.samples:
                        samples[sig].append(json.dumps(d[args.field], ensure_ascii=False))
        except OSError:
            pass

    print(f"FIELD       : {args.field}")
    print(f"ENTRY TYPE  : {args.entry_type}")
    print(f"SCANNED     : {total} entries")
    print(f"WITH FIELD  : {with_field}  ({100*with_field/max(total,1):.2f}%)")
    print(f"UNIQUE SHAPES: {len(shapes)}")
    print()
    if not shapes:
        print("(no occurrences found)")
        return

    print(f"--- top {min(args.top, len(shapes))} shapes ---")
    for sig, c in shapes.most_common(args.top):
        pct = 100 * c / max(with_field, 1)
        print(f"  {c:>6}  ({pct:5.2f}%)  {sig}")

    if args.samples:
        print()
        print(f"--- samples (up to {args.samples} per shape) ---")
        for sig, c in shapes.most_common(args.top):
            print()
            print(f"[{c} hits] {sig}")
            for ex in samples.get(sig, []):
                if len(ex) > args.sample_len:
                    ex = ex[:args.sample_len] + " ...[truncated]"
                print(f"  {ex}")

if __name__ == "__main__":
    main()
