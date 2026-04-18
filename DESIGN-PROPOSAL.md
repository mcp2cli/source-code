# Design Proposal: Discovery-Driven Domain-Native CLI

> **Status: Implemented.** Phases 1–7 are complete. The dynamic CLI surface is
> live, the protocol-shaped commands remain as backward-compatible fallbacks,
> and all 122 tests pass (83 unit + 39 integration). Phase 2 adds MCP
> 2025-11-25 protocol features (tasks, cancellation, subscriptions, roots,
> progress tokens).

## Problem

The original CLI surface was protocol-shaped:

```bash
work tool call echo --arg message=hello
work resource read demo://resource/readme.md
work prompt run simple-prompt --arg context.thread_id=123
```

This exposes MCP's internal taxonomy (tool/resource/prompt) to the user.
The RFC vision is that each aliased app should feel like its own real CLI:

```bash
work echo --message hello
work get demo://resource/readme.md
work simple-prompt --context.thread_id 123
```

The user shouldn't know or care whether `echo` is a tool, `readme.md` is a
resource, or `simple-prompt` is a prompt. They're all just **commands**.

---

## Design Principles

1. **Server capabilities ARE the command tree.** Tools become verbs, resources
   become nouns, prompts become workflows — all at the same level.
2. **JSON Schema drives flags.** Tool `inputSchema` and prompt `arguments`
   generate real typed `--flags`, not generic `--arg key=value`.
3. **No MCP jargon in public UX.** No `tool`, `resource`, `prompt` as
   user-visible command names. Those become internal routing.
4. **Every alias feels like a different app.** `email` has `send`, `labels`,
   `inbox`. `weather` has `forecast`, `alerts`. Same runtime, different face.
5. **Zero-config for any MCP server.** The generic dynamic mode works out of
   the box. Optional per-server profiles can refine naming and grouping.

---

## Architecture: Two-Layer Command Surface

### Layer 1: Dynamic command surface (zero-config)

The runtime discovers server capabilities and **synthesizes a CLI** from them.
No hardcoded command names. The server's metadata IS the help text.

### Layer 2: Profile overlays (optional refinement)

A per-server YAML profile can rename, group, hide, or alias commands.
This turns a generic dynamic CLI into something that feels hand-crafted.

Both layers use the same execution engine underneath.

---

## How It Works

### Step 1: Discovery → Command Manifest

On first interaction (or from cache), the runtime fetches:
- `tools/list` → tools with `name`, `description`, `inputSchema`
- `resources/list` → resources with `uri`, `name`, `description`, `mimeType`
- `resourceTemplates/list` → templates with `uriTemplate`, `name`, `description`
- `prompts/list` → prompts with `name`, `description`, `arguments[]`

It builds a **Command Manifest** — a merged, deduplicated command tree:

```yaml
# auto-generated from server-everything
commands:
  # --- from tools ---
  echo:
    kind: tool
    summary: "Echoes back the input"
    flags:
      message: { type: string, required: true, help: "Message to echo" }

  add:
    kind: tool
    summary: "Adds two numbers"
    flags:
      a: { type: number, required: true }
      b: { type: number, required: true }

  long-running-operation:
    kind: tool
    summary: "Demonstrates progress notifications"
    flags:
      duration: { type: integer, default: 10 }
      steps: { type: integer, default: 5 }
    supports_background: true

  sample-llm:
    kind: tool
    summary: "Samples from an LLM"
    flags:
      prompt: { type: string, required: true }
      max-tokens: { type: integer, default: 100 }

  annotated-message:
    kind: tool
    summary: "Demonstrates annotated messages"
    flags:
      message-type: { type: string, enum: [error, success, debug], required: true }
      include-image: { type: boolean, default: false }

  get-tiny-image:
    kind: tool
    summary: "Returns a tiny test image"
    flags: {}

  print-env:
    kind: tool
    summary: "Print server environment variables"
    flags: {}

  # --- from resources ---
  get:
    kind: resource
    summary: "Fetch a resource by URI"
    positional: uri
    # concrete resources become completions/suggestions for the URI arg

  # --- from resource templates ---
  # templates with single parameter hoist the param as positional
  # templates with multiple params become flags

  # --- from prompts ---
  simple-prompt:
    kind: prompt
    summary: "A simple prompt without arguments"
    flags: {}

  complex-prompt:
    kind: prompt
    summary: "A complex prompt with arguments"
    flags:
      temperature: { type: number, required: true }
      style: { type: string, required: true }
```

### Step 2: Dynamic CLI Generation

The manifest becomes a real clap command tree at runtime:

```
$ work --help
work 0.1.0 — MCP bridge for "server-everything"

USAGE: work <COMMAND> [OPTIONS]

COMMANDS:
  echo                  Echoes back the input
  add                   Adds two numbers
  long-running-operation  Demonstrates progress notifications
  sample-llm            Samples from an LLM
  annotated-message     Demonstrates annotated messages
  get-tiny-image        Returns a tiny test image
  print-env             Print server environment variables
  get                   Fetch a resource by URI
  simple-prompt         A simple prompt without arguments
  complex-prompt        A complex prompt with arguments

RUNTIME:
  auth                  Authentication management
  jobs                  Background job management
  doctor                Runtime health diagnostics
  inspect               Full server metadata and capabilities

FLAGS:
  --json                Output in JSON format
  --background          Run as background job (tools only)
  --non-interactive     Fail instead of prompting (CI mode)
  -h, --help            Print help
  -V, --version         Print version
```

Each command has proper typed help:

```
$ work echo --help
Echoes back the input

USAGE: work echo --message <MESSAGE>

OPTIONS:
  --message <MESSAGE>    Message to echo [required]
  --json                 Output in JSON format
  --background           Run as background job
  -h, --help             Print help
```

```
$ work add --help
Adds two numbers

USAGE: work add --a <NUMBER> --b <NUMBER>

OPTIONS:
  --a <NUMBER>    [required]
  --b <NUMBER>    [required]
  --json          Output in JSON format
  -h, --help      Print help
```

### Step 3: Execution Routing

When the user runs `work echo --message hello`, the runtime:

1. Looks up `echo` in the manifest → `kind: tool`
2. Validates `--message` against the schema
3. Constructs `McpOperation::InvokeAction { capability: "echo", arguments: {"message": "hello"} }`
4. Dispatches through the existing MCP client
5. Renders the result

When the user runs `work get demo://resource/readme.md`:

1. Looks up `get` → `kind: resource`
2. Takes positional arg as URI
3. Constructs `McpOperation::ReadResource { uri: "..." }`
4. Dispatches, renders

When the user runs `work simple-prompt`:

1. Looks up `simple-prompt` → `kind: prompt`
2. No flags needed
3. Constructs `McpOperation::RunPrompt { name: "simple-prompt", arguments: {} }`
4. Dispatches, renders

---

## Resource Command Design

Resources need careful treatment because they're nouns, not verbs.

### Concrete resources → `get` command

All concrete resources are fetchable via a unified `get` command:

```bash
work get demo://resource/readme.md
work get mail://inbox/123
work get file:///etc/hosts
```

The `get` verb is familiar from `curl`, `wget`, `http get`. It means
"fetch this thing." Concrete resource URIs from discovery become shell
completions and suggestions in `--help`.

### Resource templates → parameterized commands

Templates with meaningful names become their own commands:

```yaml
# Server exposes template: greeting/{name}
# Auto-generates:
greeting:
  kind: resource_template
  summary: "A personalized greeting"
  positional: name

# User runs:
work greeting Alice
# → materializes URI greeting/Alice → resources/read
```

For templates with multiple parameters:

```yaml
# Server exposes template: mail://search?query={query}&folder={folder}
mail-search:
  kind: resource_template
  summary: "Search mail"
  flags:
    query: { type: string, required: true }
    folder: { type: string }

# User runs:
work mail-search --query invoice --folder inbox
```

### `ls` — list available resources

```bash
work ls                    # list all resources, templates, tools, prompts
work ls --tools            # just tools
work ls --resources        # just resources
work ls --prompts          # just prompts
work ls --filter echo      # filter by name substring
```

This replaces `tool list`, `resource list`, `prompt list` with a single
unix-familiar `ls` command. `ls` is read-only exploration.

---

## Namespace Grouping (Dotted Names → Subcommands)

Many MCP servers use dotted or slash-separated tool names for grouping:

```
email.send
email.reply
email.draft.create
email.draft.list
email.labels.list
email.labels.add
```

The runtime detects common prefixes and auto-generates subcommand groups:

```
$ email --help

COMMANDS:
  send            Send an email
  reply           Reply to an email
  draft           Draft management
    create          Create a draft
    list            List drafts
  labels          Label management
    list            List labels
    add             Add a label
  get             Fetch a resource by URI
  auth            Authentication
  jobs            Background jobs
  doctor          Runtime diagnostics
```

Grouping rules:
- If ≥2 capabilities share a prefix, create a subcommand group
- Single capabilities with a prefix keep the full name as-is
- The shared prefix becomes the group; suffixes become subcommands
- Dots, slashes, underscores, and hyphens are all treated as separators

---

## Profile Overlays (Optional Per-Server Customization)

For users who want a polished, curated feel, a profile YAML in the config
directory can override auto-generated names:

```yaml
# configs/work.profile.yaml — optional overlay
display_name: "Work Server"
aliases:
  echo: ping                    # rename "echo" → "ping"
  long-running-operation: lro   # short alias
  get-tiny-image: image         # friendlier name
hide:
  - print-env                   # hide from help and ls
  - annotated-message           # internal/debug tool
groups:
  debug:                        # custom grouping
    - print-env
    - annotated-message
    - get-tiny-image
flags:
  echo:
    message: msg                # rename --message → --msg
resource_verb: fetch            # use "fetch" instead of "get"
```

Result:

```
$ work --help
work 0.1.0 — Work Server

COMMANDS:
  ping              Echoes back the input
  add               Adds two numbers
  lro               Demonstrates progress notifications
  sample-llm        Samples from an LLM
  image             Returns a tiny test image
  fetch             Fetch a resource by URI
  simple-prompt     A simple prompt without arguments
  complex-prompt    A complex prompt with arguments
  debug             Debug tools
  auth              Authentication management
  jobs              Background job management
  doctor            Runtime health diagnostics

$ work ping --msg hello
Echo: hello
```

Profiles are **always optional**. Without one, the dynamic surface works
as-is from server metadata alone.

---

## Unified Argument Handling

### Schema-derived typed flags

Tool `inputSchema` generates typed `--flag` arguments:

| JSON Schema type | CLI flag type | Example |
|-----------------|--------------|---------|
| `string` | `--name <VALUE>` | `--message hello` |
| `integer` | `--count <INT>` | `--steps 5` |
| `number` | `--rate <NUM>` | `--temperature 0.7` |
| `boolean` | `--flag` (no value) | `--include-image` |
| `enum` values | `--kind <A\|B\|C>` | `--message-type error` |
| `array` | `--tags <V1,V2,...>` | `--labels bug,urgent` |

Required properties become required flags. Optional properties have defaults
shown in help.

### Argument precedence (same as current)

1. `--args-file` (base layer from file)
2. `--args-json` (JSON string overlay)
3. `--arg-json` (per-key JSON)
4. `--flag` / direct flags (final override)

Direct flags (from schema) and generic `--arg`/`--arg-json` coexist. You can
always fall back to `--arg key=value` for capabilities where schema isn't
cached or for edge cases.

### Prompt arguments → flags

Prompt `arguments` array generates flags just like tool schemas:

```json
{"name": "complex-prompt", "arguments": [
  {"name": "temperature", "required": true},
  {"name": "style", "required": true}
]}
```

Becomes:
```
$ work complex-prompt --temperature 0.7 --style concise
```

---

## The `get` / `fetch` Verb and URI Completion

The unified resource verb (`get` by default, configurable via profile) handles
all resource reads. It uses shell completion from cached discovery:

```bash
# TAB completion shows known resources
$ work get <TAB>
demo://resource/static/document/architecture.md
demo://resource/static/document/readme.txt
demo://resource/static/data/catalog.json

# Also handles template expansion inline
$ work get greeting/Alice
```

When a template matches the URI pattern, the runtime materializes it. When a
concrete URI matches, it reads directly. When neither matches, it tries a
raw `resources/read` as a fallback.

---

## Runtime-Owned Commands (Always Present)

These are built-in runtime commands that exist on every alias:

| Command | Purpose |
|---------|---------|
| `auth login` | Authenticate |
| `auth logout` | Clear credentials |
| `auth status` | Show auth state |
| `jobs list` | List background jobs |
| `jobs show [ID]` | Show job details |
| `jobs wait [ID]` | Wait for completion |
| `jobs cancel [ID]` | Cancel a job |
| `jobs watch [ID]` | Watch job progress |
| `doctor` | Runtime/transport diagnostics |
| `inspect` | Full server capability dump |
| `ls` | List all commands/capabilities |

If a server tool conflicts with a runtime command name (e.g., a tool called
"auth"), the tool keeps its name and the runtime command is still accessible.
The resolution rule: runtime commands take precedence. If a tool is named
`auth`, it becomes accessible as `raw:auth` or via `--arg` fallback.

---

## Concrete Example: Email MCP Server

Imagine an email MCP server exposes:

**Tools:** `send`, `reply`, `forward`, `archive`, `labels.add`, `labels.remove`, `draft.create`, `draft.send`
**Resources:** `mail://inbox`, `mail://sent`, `mail://draft/123`, `mail://message/456`
**Resource templates:** `mail://search?q={query}`, `mail://folder/{name}`, `mail://message/{id}`
**Prompts:** `summarize-thread`, `compose-reply`, `triage-inbox`

Zero-config dynamic CLI:

```
$ email --help
email 0.1.0 — Email MCP Server

COMMANDS:
  send                Send an email
  reply               Reply to an email
  forward             Forward an email
  archive             Archive messages
  labels              Label management
    add                 Add a label
    remove              Remove a label
  draft               Draft management
    create              Create a draft
    send                Send a draft
  get                 Fetch a resource by URI
  search              Search messages
  folder              View a mail folder
  message             View a message by ID
  summarize-thread    Summarize an email thread
  compose-reply       Compose a reply with AI
  triage-inbox        Triage your inbox
  auth                Authentication
  jobs                Background jobs
  doctor              Runtime diagnostics
  ls                  List all capabilities

$ email send --to user@example.com --subject "Hello" --body "Hi there"
✓ Message sent

$ email labels add --message-id 456 --label urgent
✓ Label added

$ email get mail://inbox
[inbox contents]

$ email search --query invoice
[search results]

$ email message 456
[message content]

$ email summarize-thread --thread-id th_123 --style concise
[summary]

$ email triage-inbox
[triage results]

$ email send --to user@example.com --subject "Report" --background
Job created: j_8f2d
$ email jobs watch j_8f2d
```

With a profile overlay, this could be refined further — but it already
feels like a real email CLI without any profile file.

---

## Concrete Example: Generic server-everything

Zero-config against `@modelcontextprotocol/server-everything`:

```
$ work echo --message hello
Echo: hello

$ work add --a 5 --b 3
The sum of 5 and 3 is 8.

$ work long-running-operation --duration 5 --steps 3
[progress: step 1/3]
[progress: step 2/3]
[progress: step 3/3]
Done.

$ work get-tiny-image
[image data]

$ work simple-prompt
This is a simple prompt without arguments.

$ work complex-prompt --temperature 0.7 --style concise
[prompt output]

$ work ls
TOOLS:
  echo                    Echoes back the input
  add                     Adds two numbers
  long-running-operation  Demonstrates progress notifications
  sample-llm              Samples from an LLM
  annotated-message       Demonstrates annotated messages
  get-tiny-image          Returns a tiny test image
  print-env               Print server environment variables

RESOURCES:
  demo://resource/static/document/architecture.md
  demo://resource/static/document/readme.txt
  ...

PROMPTS:
  simple-prompt           Simple prompt
  complex-prompt          Complex prompt with arguments

$ work inspect
[full capability view]
```

---

## Concrete Example: GitHub MCP Server

Hypothetical GitHub MCP server exposing tools like `repos.list`, `repos.create`,
`issues.list`, `issues.create`, `issues.comment`, `pr.list`, `pr.review`, etc.:

```
$ gh --help
gh 0.1.0 — GitHub via MCP

COMMANDS:
  repos               Repository management
    list                List repositories
    create              Create a repository
  issues              Issue management
    list                List issues
    create              Create an issue
    comment             Comment on an issue
  pr                  Pull request management
    list                List pull requests
    review              Review a pull request
  auth                Authentication
  jobs                Background jobs
  doctor              Runtime diagnostics
  ls                  List capabilities

$ gh repos list --org my-org --limit 10
$ gh issues create --repo owner/repo --title "Bug" --body "Details..."
$ gh pr list --repo owner/repo --state open
```

---

## Implementation Plan

> All phases are implemented. The sections below document the original plan
> with completion notes.

### Phase 1: Rich Discovery Cache ✅

**What:** Preserve full `inputSchema` and `uriTemplate` in discovery cache.

- Modify `map_discovery_response()` in client.rs to retain `inputSchema` on
  tools and `uriTemplate` on resource templates
- Extend `DiscoveryInventoryView` to store the richer data
- Extend prompt items to include argument metadata (type, required, description)

**Why:** Everything else depends on having schema data available at CLI
generation time.

**Risk:** Low — additive change to cache, no behavior change.

### Phase 2: Command Manifest Builder ✅

**What:** New module `apps/manifest.rs` that transforms discovery cache into
a `CommandManifest`.

```rust
struct CommandManifest {
    commands: IndexMap<String, ManifestCommand>,
}

struct ManifestCommand {
    kind: CommandKind,              // Tool, Resource, ResourceTemplate, Prompt
    name: String,                   // CLI command name
    summary: String,                // From description
    flags: IndexMap<String, FlagSpec>,
    positional: Option<PositionalSpec>,
    supports_background: bool,
}

struct FlagSpec {
    flag_type: FlagType,            // String, Integer, Number, Boolean, Enum, Array
    required: bool,
    default: Option<Value>,
    help: Option<String>,
    enum_values: Option<Vec<String>>,
}
```

The builder handles:
- Name normalization (dots/slashes → subcommand groups)
- Schema → flag conversion
- Template URI → positional/flag conversion
- Conflict detection with runtime commands

**Risk:** Medium — core new logic, but isolated in its own module.

### Phase 3: Dynamic clap Generation ✅

**What:** Replace the static `BridgeCli` parser with a dynamic clap builder
that generates commands from the manifest.

- Use `clap::Command::new()` builder API instead of `#[derive(Parser)]`
- Generate subcommands from manifest entries
- Generate `--flag` args from `FlagSpec`
- Generate help text from manifest metadata
- Handle subcommand groups for namespaced commands
- Keep runtime commands (auth, jobs, doctor, etc.) as static additions

**Risk:** Medium-high — replaces the current parsing layer. The old static
parser becomes a fallback for when no discovery cache exists yet.

### Phase 4: Execution Router ✅

**What:** Route dynamically-parsed commands back to MCP operations.

- Match parsed command name → manifest entry
- Extract flag values → construct `arguments` JSON object
- Based on `kind`: dispatch to tool call, resource read, or prompt get
- Handle `--background` for tools
- Handle positional URI for `get` command
- Handle template materialization for resource templates

**Risk:** Low-medium — maps directly to existing `McpOperation` variants.

### Phase 5: Profile Overlays ✅

**What:** Optional YAML profiles for per-server customization.

- `configs/<name>.profile.yaml` alongside `configs/<name>.yaml`
- Rename, alias, hide, group commands
- Rename flags
- Custom display_name for help banner
- Applied as a transform over the auto-generated manifest

**Risk:** Low — purely additive refinement layer.

### Phase 6: `ls` and Shell Completion ✅

**What:** Unified listing and shell tab completion from manifest.

- `ls` command replaces `tool list` / `resource list` / `prompt list`
- Shell completion generation from manifest (bash, zsh, fish)
- URI completion for `get` command from cached resources

**Risk:** Low — UX polish.

### Phase 7: Backward Compatibility ✅

**What:** Keep hidden protocol-oriented fallbacks.

- `--arg key=value` still works alongside typed flags
- `tool call`, `resource read`, `prompt run` remain as hidden aliases
- `discover`, `invoke`, `read`, `list` remain as hidden aliases
- If no discovery cache exists, fall back to protocol-shaped commands

**Risk:** Low — preservation of existing behavior.

---

## Migration Path (completed)

| Phase | User sees | Internally |
|-------|----------|------------|
| Phase 1-2 | `work ls` with richer cache | Cache preserves schemas |
| Phase 3-4 | `work echo --message hello` | Dynamic clap from manifest |
| Phase 5 | Custom profiles refine naming | Overlay transforms |
| Phase 6 | TAB completion, unified `ls` | Shell integration |
| Phase 7 | Consistent domain-native surface | Backward compat hidden aliases |

---

## Key Design Decisions to Validate

1. **`get` vs `fetch` vs `read` for resources** — `get` is most unix-like
   (`curl`, `wget`, `http get`), but `fetch` is also natural. `read` sounds
   like file I/O. Recommend `get` as default, overridable via profile.

2. **Prompt commands at top level** — prompts become top-level commands
   alongside tools. They're actions the user runs. No separate "prompt"
   namespace unless the user adds a profile group.

3. **Template commands vs `get` with smart resolution** — simple templates
   (single param) become their own named commands. Complex templates become
   flag-driven commands. All are also reachable via `get <materialized-uri>`.

4. **Conflict resolution** — if a tool is named `auth`, `jobs`, `doctor`, or
   `ls`, runtime commands win. The tool is accessible as `raw:auth` or via the
   generic `--arg` fallback. In practice this is rare.

5. **`ls` vs `list`** — `ls` is more unix-native and shorter. `list` is
   available as an alias.

6. **Flag casing** — tool schema properties like `maxTokens` become
   `--max-tokens` (kebab-case). Properties like `message_type` become
   `--message-type`. Standard CLI convention.

---

## What This Achieves

| Goal | Status |
|------|--------|
| Feels like a real CLI | ✅ `work echo --message hello` |
| Typed flags from JSON Schema | ✅ `--message <STRING>`, `--count <INT>` |
| Self-documenting | ✅ Help generated from server metadata |
| Each alias unique | ✅ Commands reflect server's capabilities |
| Zero-config | ✅ Works with any MCP server |
| Refineable | ✅ Optional profile overlays |
| Shell completion | ✅ From manifest |
