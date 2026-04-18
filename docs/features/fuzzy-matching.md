# Fuzzy Command Matching

Mistype a command? mcp2cli suggests the closest match instead of a cryptic error.

---

## How It Works

When you type a command that doesn't exist, mcp2cli computes the [Levenshtein edit distance](https://en.wikipedia.org/wiki/Levenshtein_distance) between your input and all known commands from the server's manifest. If a close match is found, you get a helpful suggestion:

```shellsession
$ work ecoh --message hello
error: unrecognized subcommand 'ecoh'

Did you mean: echo?
```

```shellsession
$ work serch --query "test"
error: unrecognized subcommand 'serch'

Did you mean: search?
```

---

## Matching Rules

| Rule | Detail |
|------|--------|
| **Max distance** | Suggestions are shown for edit distance ≤ 3 and < half the query length + 1 |
| **Max suggestions** | Up to 3 closest matches are shown |
| **Sorting** | Closest match first |
| **Scope** | Only manifest commands are matched (server tools, resources, prompts) |

### What Counts as an Edit?

Levenshtein distance counts the minimum number of:
- **Insertions** — `ech` → `echo` (1 insertion)
- **Deletions** — `eccho` → `echo` (1 deletion)
- **Substitutions** — `ecxo` → `echo` (1 substitution)

### Examples

| Typed | Actual | Distance | Suggested? |
|-------|--------|----------|------------|
| `ecoh` | `echo` | 2 | ✅ |
| `serch` | `search` | 1 | ✅ |
| `deplpy` | `deploy` | 2 | ✅ |
| `xyz` | `echo` | 4 | ❌ (too far) |
| `e` | `echo` | 3 | ✅ (distance ≤ 3) |

---

## Multiple Suggestions

When several commands are close, all are listed:

```shellsession
$ work ad --a 1 --b 2
error: unrecognized subcommand 'ad'

Did you mean: add, audit, admin?
```

---

## Runtime Commands Are Not Matched

Fuzzy matching applies only to **server-derived commands** (tools, resources, prompts). Built-in runtime commands like `ls`, `ping`, `auth`, `jobs`, `doctor` are not included in the fuzzy search because they're handled by a separate parser.

---

## Interaction with Profile Overlays

Fuzzy matching works against the **final manifest** — after profile aliases, renames, and groups have been applied. If you've renamed `echo` to `ping` via a profile overlay, the fuzzy matcher suggests `ping`, not `echo`.

---

## See Also

- [Discovery-Driven CLI](discovery-driven-cli.md) — how the command manifest is built
- [Profile Overlays](profile-overlays.md) — command renaming affects fuzzy results
