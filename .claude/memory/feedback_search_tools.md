---
name: feedback_search_tools
description: Use rg and fd instead of grep and find — they're faster and always available in this project
metadata:
  type: feedback
---

Always use `rg` instead of `grep` and `fd` instead of `find`.

**Why:** Both are installed, faster, respect `.gitignore`, and the user has corrected this explicitly.

**How to apply:** Every time you'd reach for `grep` or `find`, use `rg` or `fd` instead. Use `rg -P` for PCRE2 (lookaheads/backreferences). Use `fd -e EXT` to filter by extension, `fd -t f`/`-t d` for type.
