# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
import collections
import os
import re
import sys
import xml.etree.ElementTree as ET

report, name = sys.argv[1], sys.argv[2]
allowed = re.compile(os.environ.get("ALLOWED") or "^$")
skip_line = re.compile(r"^\s*skipping\b")
cases = ET.parse(report).getroot().iter("testcase")
per_binary = collections.Counter()
outcomes = collections.Counter()
unexpected_skips = []
for case in cases:
    per_binary[case.get("classname")] += 1
    outcome = next((c.tag for c in case if c.tag in ("failure", "error", "skipped")), "passed")
    outcomes[outcome] += 1
    out = "".join(c.text or "" for c in case if c.tag == "system-out")
    for line in out.splitlines():
        if skip_line.match(line) and not allowed.search(line):
            unexpected_skips.append(f"{case.get('classname')} {case.get('name')}: {line.strip()}")

print(f"### {name}: {sum(per_binary.values())} tests, " + ", ".join(f"{n} {k}" for k, n in sorted(outcomes.items())))
print()
print("| binary | tests |")
print("|---|---|")
for binary, n in sorted(per_binary.items()):
    print(f"| {binary} | {n} |")
for skip in unexpected_skips:
    print(f"::error::did not run: {skip}")
sys.exit(1 if unexpected_skips else 0)
