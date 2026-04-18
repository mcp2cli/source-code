# mcp2cli — Channel Playbooks

Detailed execution scripts for every channel. Copy-paste ready where possible.

---

## YouTube

### Channel Setup

- **Channel name**: `TSOK AI Lab` (umbrella) or `mcp2cli` (dedicated)
- **Recommendation**: Start as `TSOK AI Lab` to build lab brand; create a playlist for mcp2cli
- **Channel art**: Terminal-aesthetic, dark background, TSOK logo
- **About section**:
  > TSOK — The Source of Knowledge AI Laboratory. We build open-source developer tools for the AI ecosystem. mcp2cli and more.

### Video Descriptions Template

Use this template for all 6 videos:

```markdown
[Video-specific opening paragraph — 2-3 sentences summarising what viewers will learn]

🔗 GitHub: https://github.com/mcp2cli/source-code?utm_source=youtube&utm_medium=video&utm_campaign=launch-month1&utm_content=[VIDEO-ID]
📖 Documentation: https://[docs-site]?utm_source=youtube
💬 Discord: [discord-link]
🐦 Follow TSOK AI Lab: https://x.com/tsok_lab

---

CHAPTERS
00:00 Introduction  
[chapter timestamps]

---

ABOUT mcp2cli
mcp2cli turns any MCP server into a native command-line application. 
Server tools become verbs, resources become nouns, prompts become workflows — 
no MCP protocol knowledge required.

Built at TSOK — The Source of Knowledge AI Laboratory.
Open source · MIT License · Free forever

---

TAGS
mcp, model context protocol, cli, rust, developer tools, ai tools, terminal, 
open source, tsok, ai lab, mcp server, devops, platform engineering
```

### Video Scripts / Outlines

---

#### V1: "mcp2cli in 5 Minutes" — Launch Day

**Goal**: Maximum accessibility. Anyone watching should understand and want to try it.  
**Length target**: 4–5 minutes  
**Thumbnail**: "5 MINUTES" in large text · terminal screenshot behind · mcp2cli logo

```
OPENING (0:00–0:30)
Show the terminal. Type one command. Server responds. "That's it."
Hook: "Most MCP servers are amazing tools locked behind protocol plumbing. 
What if you could use any of them like a real CLI?"

PROBLEM (0:30–1:00)
Show what it takes without mcp2cli: JSON-RPC, session negotiation, custom code.
"Every time you want to test an MCP server, you write client code. 
Every. Single. Time. Until now."

INSTALL (1:00–1:30)
cargo install mcp2cli  [show this, it runs fast]

DEMO — AD HOC (1:30–2:30)
mcp2cli --url http://localhost:3001/mcp ls
[capabilities appear as a table]
mcp2cli --url http://localhost:3001/mcp echo --message "hello from cli"
[result returns]
"No config. No setup. Just point and go."

DEMO — NAMED CONFIG (2:30–3:30)
mcp2cli config init --name work --transport streamable_http --endpoint http://...
mcp2cli link create --name work
work ls
work echo --message hello
"Now it's a first-class CLI. Tab completion. Typed flags. Help text."

DEMO — JSON OUTPUT (3:30–4:00)
work --json ls | jq '.data.tools[].name'
"Perfect for scripts, CI/CD, and AI agents."

CLOSE (4:00–4:30)
"mcp2cli is free, open source, MIT. Built at TSOK AI Lab.
Star the repo, join the Discord, and turn your MCP server into a real CLI."
[Show GitHub stars counter if > 0]
```

---

#### V2: "Testing MCP Servers with mcp2cli — CI/CD Validation"

**Goal**: Convince MCP server authors this is their canonical test client.  
**Length target**: 7–10 minutes  
**Thumbnail**: "ZERO CODE CI/CD" · green check mark · terminal

```
OPENING: Problem — every MCP server author writes a test harness. This replaces it.
SECTION 1: Install mcp2cli
SECTION 2: Test against a running MCP server (health, capabilities, tool calls)
SECTION 3: JSON output mode — pipe to jq for assertions
SECTION 4: GitHub Actions CI/CD example:
  - Start server in background
  - mcp2cli doctor (health check)
  - mcp2cli ls --json | jq assertion
  - mcp2cli invoke echo --json
SECTION 5: Error handling — what happens when server is broken
CLOSE: "Replace your test client, or never write one. Your choice."
```

---

#### V3: "mcp2cli + AI Agents: Tool Use from Shell"

**Goal**: Reach the AI agent developer audience.  
**Length target**: 8–12 minutes  
**Thumbnail**: "AI AGENTS × CLI" · code snippet

```
OPENING: "AI agents need to call tools. Most use Python SDKs. 
What if the tool interface was just... a shell command?"
SECTION 1: The --json output format (show the structured envelope)
SECTION 2: Calling an MCP tool from a bash script
SECTION 3: Calling an MCP tool from a Python agent loop (LangChain/LangGraph style)
SECTION 4: Background jobs — long-running tool, check status, get result
SECTION 5: Chaining: MCP tool output → stdin of next command
CLOSE: "MCP tools are now shell commands. Your agent is now composable."
```

---

#### V4: "Platform Engineering with mcp2cli"

**Goal**: Reach SRE/DevOps/platform engineers.  
**Length target**: 10–12 minutes  
**Thumbnail**: "PLATFORM ENGINEERING" · infrastructure diagram

```
OPENING: "Your infrastructure has an MCP server. mcp2cli is its CLI."
SECTION 1: Deploy health gates — doctor in deployment pipeline
SECTION 2: Multi-server wiring (multiple symlinks, multiple configs)
SECTION 3: Event system — streaming events from operations
SECTION 4: Daemon mode — persistent connections for low-latency automation
SECTION 5: Real-world: Kubernetes status tool wrapped as k8s CLI
CLOSE: "Stop writing bespoke infrastructure tooling. Wire up MCP."
```

---

#### V5: "Build & Test an MCP Server in 5 Minutes"

**Goal**: Hands-on coding. Attract the builders.  
**Length target**: 12–15 minutes  
**Thumbnail**: "BUILD AN MCP SERVER" · code + terminal split

```
OPENING: "I'm going to build a simple MCP server from scratch and 
test every endpoint without writing a single line of client code."
SECTION 1: Scaffold a tiny MCP server in Rust (or TypeScript)
SECTION 2: Implement one tool (echo)
SECTION 3: mcp2cli --stdio test from the command line, live
SECTION 4: Add a resource, test it
SECTION 5: Add a prompt, test it
SECTION 6: Write the GitHub Actions test config
CLOSE: "From zero to tested MCP server in under 15 minutes."
```

---

#### V6: "Advanced mcp2cli — Daemon Mode, Jobs, Event Streaming"

**Goal**: Retain and deepen engagement with existing users.  
**Length target**: 10–15 minutes  
**Thumbnail**: "ADVANCED" badge · event stream scrolling

```
OPENING: "You've used mcp2cli for the basics. Now let's go deep."
SECTION 1: Daemon mode — what it is, when to use it, how it speeds up repeated calls
SECTION 2: Background jobs — --background flag, mcp2cli jobs list, jobs watch
SECTION 3: Event system — stdio events, HTTP webhooks, SSE streams, Unix sockets
SECTION 4: Telemetry — what we collect, how to opt out (transparency builds trust)
SECTION 5: Profile overlays — renaming, hiding, grouping commands
SECTION 6: Auth flows — OAuth, token storage
CLOSE: "mcp2cli is deeper than it looks. Dig in."
```

---

### YouTube SEO — Tags to use on all videos

```
mcp, model context protocol, mcp server, mcp client, cli tool, rust cli, 
developer tools 2026, open source ai, ai tools, terminal automation, 
devops tools, platform engineering, ai agents, llm tools, tsok, 
tsok ai lab, mcp2cli, command line interface rust
```

---

## X (Twitter)

### Account Setup

- **Handle**: `@tsok_lab` (lab account, primary) + optional `@mcp2cli`
- **Bio**: `Building open-source tools for the AI ecosystem. 🔬 TSOK AI Lab. Latest: mcp2cli → github.com/mcp2cli/source-code`
- **Pinned tweet**: The launch thread (pin it on Day 0)

---

### S1: Main Launch Thread — Day 0

Post at 09:15 PST. Draft all 8 tweets before posting. Publish #1, then the rest as replies.

---

**Tweet 1 / 8** — Hook
```
🧵 We just open-sourced mcp2cli at @tsok_lab.

Turn any MCP server into a native CLI in seconds: 
→ auto-discovered tools become typed --flags
→ resources become `get` commands  
→ no JSON-RPC, no protocol knowledge

Here's the 90-second demo, and why we built it 👇

1/8
```
*(Attach: demo GIF)*

---

**Tweet 2 / 8** — Problem
```
The problem: every MCP server author has to write a custom client to test their server.

Every. Single. Time.

And every DevOps team that wants CLI access to an MCP-backed tool? 
Same thing.

We were tired of writing the same plumbing over and over.

2/8
```

---

**Tweet 3 / 8** — Solution: Zero config
```
So we built mcp2cli.

Zero config — just point at a server:

```
mcp2cli --url http://localhost:3001/mcp ls

mcp2cli --url http://localhost:3001/mcp echo \
  --message "hello world"
```

Capabilities are auto-discovered.
Flags are typed from JSON Schema.
Results are human-readable or --json.

3/8
```

---

**Tweet 4 / 8** — Named configs + aliases
```
For permanence, create a named config:

```
mcp2cli config init --name work \
  --transport streamable_http \
  --endpoint http://server:3001/mcp

mcp2cli link create --name work
```

Then it's just:

```
work ls
work echo --message hello
work get file:///README.md
```

One binary. Any server. Real commands.

4/8
```

---

**Tweet 5 / 8** — CI/CD / programmatic use
```
Pipes. Exit codes. --json output. --timeout. --background.

```
work --json doctor | jq '.data.server.version'

staging deploy --version 2.1.0 --background \
  && staging jobs watch
```

It's a first-class shell citizen.
Designed for CI/CD, automation, and AI agents.

5/8
```

---

**Tweet 6 / 8** — TSOK Lab brand
```
We built this at TSOK — The Source of Knowledge AI Laboratory.

Our thesis: the AI ecosystem moves fast, but the developer tooling lags.

mcp2cli is the first of many tools we're releasing to fix that.

Free. Open source. MIT. Forever.

6/8
```

---

**Tweet 7 / 8** — Social proof / credibility
```
What's under the hood:

→ Written in Rust 🦀
→ MCP 2025-11-25 spec compliant
→ 135 tests (96 unit + 39 integration)
→ Streamable HTTP + Stdio transports
→ Daemon mode, background jobs, event streaming, OAuth auth
→ 13 feature guides, 8 use-case articles in the docs

This isn't a prototype.

7/8
```

---

**Tweet 8 / 8** — CTA
```
⭐️ Star on GitHub: github.com/mcp2cli/source-code

📖 Docs: [docs-link]

If you build MCP servers, test with mcp2cli.
If you deploy AI tools, automate with mcp2cli.
If you want to ship open-source AI tooling — follow @tsok_lab.

What MCP server will you wire up first? 👇

8/8
```

---

### Additional X Content Templates

**Engagement tweet (Day 5)**
```
Question for #MCP builders:

What's the most annoying part of testing your MCP server?

→ Writing client code every time
→ No standard test tooling
→ JSON-RPC debugging is painful
→ No way to call it from CI/CD

(mcp2cli exists for all of these, but I want to hear the raw pain 👇)
```

**Mini-tutorial thread (Day 20) — CI/CD in 3 commands**
```
Here's the exact 3-command MCP server CI/CD setup I use 🧵

```
# In GitHub Actions:

# 1. Health check
mcp2cli --url $MCP_URL doctor

# 2. Validate capabilities exist
mcp2cli --url $MCP_URL ls --json | \
  jq -e '.data.tools | map(.name) | contains(["echo"])'

# 3. Run a smoke test
mcp2cli --url $MCP_URL echo --json \
  --message "ci-smoke-test" | \
  jq -e '.data.content[0].text == "ci-smoke-test"'
```

No custom client code.
GitHub: github.com/mcp2cli/source-code

#MCP #DevOps #CI #OpenSource
```

---

## Hacker News

### Show HN Post — Day 0, 09:00 PST

**Title** (exact — HN is strict about title format):
```
Show HN: mcp2cli – Turn any MCP server into a native command-line app
```

**Body text**:
```
Hi HN,

We've been building tools on Model Context Protocol (MCP) and kept hitting 
the same wall: every time you want to test an MCP server, you write a custom 
client. Every single time.

mcp2cli solves this. Point it at any MCP server and it auto-discovers the 
capabilities (tools, resources, prompts) and generates a typed CLI from the 
JSON Schema. No config required for ad hoc use:

  mcp2cli --url http://localhost:3001/mcp ls
  mcp2cli --url http://localhost:3001/mcp echo --message hello

For permanent bindings, you create a named config and symlink it:

  mcp2cli config init --name work ...
  mcp2cli link create --name work
  work echo --message hello

It's designed for real workflows: --json output, --timeout, --background jobs, 
exit codes, pipes, daemon mode, event streaming, OAuth auth.

Written in Rust. 135 tests. MCP 2025-11-25 spec compliant. MIT.

GitHub: https://github.com/mcp2cli/source-code

Happy to answer questions about the design, the MCP spec gaps we ran into, 
or the Rust implementation decisions.

– team @ TSOK AI Lab
```

**Comment responses to prepare for:**
- "What is MCP?" → have a 2-sentence answer ready
- "How is this different from [X]?" → know the landscape
- "Why Rust?" → performance, single binary, correctness
- "Is it maintained?" → yes, active, 135 tests, telemetry for usage data
- "What's TSOK AI Lab?" → have the short answer ready

---

## Product Hunt

### Submission Details

**Name**: mcp2cli  
**Tagline** (60 chars max):
```
Turn any MCP server into a native CLI in seconds
```

**Description** (260 chars max):
```
mcp2cli auto-discovers MCP server capabilities and generates a typed CLI 
from JSON Schema. No protocol knowledge needed. One binary, any server, 
real commands. For testing, CI/CD, and AI agent automation. Free & open source.
```

**Topics**: Developer Tools, CLI, AI, Open Source, Automation

**Gallery** (prepare 4–6 assets):
1. Hero GIF: terminal demo (ad hoc → named config → json output)
2. Screenshot: `work ls` output (capability table)
3. Screenshot: `work echo --message hello` (tool invocation)
4. Screenshot: `work --json doctor | jq` (CI/CD use case)
5. Screenshot: GitHub Actions CI config
6. Architecture diagram (from docs)

**Maker's first comment** (post this immediately when live):
```
Hey Product Hunt! 👋

I'm [name] from TSOK AI Lab — the team that built mcp2cli.

Quick origin story: We kept building things on MCP (Model Context Protocol) 
and every single time — building an MCP server, testing an integration, 
writing CI/CD for a deployment service — we had to write a custom client.

JSON-RPC boilerplate. Session negotiation. Protocol parsing. 

So we built mcp2cli. Turn any MCP server into a native CLI, instantly.

The "wow moment" is the first time you type:
  mcp2cli --url http://your-server/mcp ls

And your server's tools just... appear as commands.

We'd love to hear what you build. Drop questions here — we'll be responding 
all day. And if you build MCP servers, let us know and we'll add you to our 
ecosystem showcase in the docs!

GitHub: github.com/mcp2cli/source-code ⭐️
```

---

## Reddit

### r/rust — Day 0

**Title**: `[Project] mcp2cli – Turn any MCP server into a native CLI (written in Rust)`

**Body**:
```
Hey r/rust,

We just open-sourced mcp2cli — a tool written in Rust that turns any MCP 
(Model Context Protocol) server into a native command-line application.

**What it does**: Point it at any MCP server and it auto-discovers capabilities 
(tools, resources, prompts) and generates a typed CLI from the JSON Schema. 
Flags are typed, help text is auto-generated, output is structured.

**Zero config mode**:
```
mcp2cli --url http://localhost:3001/mcp ls
mcp2cli --url http://localhost:3001/mcp echo --message hello
```

**Named config + symlink mode**:
```
mcp2cli config init --name work --transport streamable_http --endpoint ...
mcp2cli link create --name work
work echo --message hello
work --json doctor | jq '.data.server'
```

**Rust specifics I'm happy to discuss**:
- Async dispatch via Tokio
- Config via figment (YAML + env overlay)
- Clap for dynamic CLI generation from discovered schema
- 135 tests (96 unit + 39 integration)
- MCP 2025-11-25 spec compliant

GitHub: https://github.com/mcp2cli/source-code

Built at TSOK AI Lab. MIT license. Happy to take questions on the implementation!
```

---

### r/commandline — Day 0

**Title**: `mcp2cli – Turn any MCP server into a real CLI app in seconds`

**Body**:
```
Something I built that I think this community will appreciate:

mcp2cli takes any MCP (Model Context Protocol) server and auto-generates 
a typed CLI from the JSON Schema it exposes. One binary. Any server.

```bash
# No config — just point and go
mcp2cli --url http://server/mcp ls

# Create a named config + symlink
mcp2cli config init --name work ...
mcp2cli link create --name work
work echo --message hello     # typed --flags from schema
work get file:///README.md    # resources too
work --json doctor | jq       # structured output
```

The symlink trick is my favourite part — `work` becomes a real CLI, 
with tab completion, typed flags, and contextual help.

GitHub: github.com/mcp2cli/source-code — free, MIT, open source.
```

---

### r/LocalLLaMA — Day 0 or Day 1

**Title**: `mcp2cli: call any MCP tool as a shell command — useful for AI agent automation`

**Body**:
```
For anyone building AI agents on top of MCP servers — I think you'll 
find this useful.

mcp2cli auto-discovers MCP server capabilities and turns them into 
native CLI commands with `--json` output. This means you can call 
MCP tools directly from agent loops, bash scripts, or LangGraph nodes:

```python
import subprocess, json

result = subprocess.run(
    ["work", "--json", "echo", "--message", user_input],
    capture_output=True
)
data = json.loads(result.stdout)
```

No MCP client library in Python. No protocol handling. Just a subprocess call.

Supports background jobs too, so long-running tools don't block:
```bash
work deploy --version 2.1.0 --background
work jobs watch  # stream status
```

GitHub: github.com/mcp2cli/source-code — free, MIT, built at TSOK AI Lab.
```

---

## Medium & dev.to

### A1 Copy: "mcp2cli: A Native CLI for Any MCP Server"

**Tags**: cli, mcp, developer-tools, openSource, rust

**Opening paragraph**:
```
If you've built an MCP (Model Context Protocol) server, you know the 
moment. You've implemented the spec, you've written your tools, your 
resources are ready — and then you need to test it. So you write a 
tiny JSON-RPC client. Or you wire up a full MCP SDK. Or you fire up 
Claude and mash through the UI.

None of those options are satisfying for a developer who wants a clean, 
repeatable test workflow.

mcp2cli exists to change that.
```

**Structure**:
1. The problem (300 words)
2. What mcp2cli does — the 5-second pitch
3. The ad-hoc mode demo
4. The named config + alias demo
5. The CI/CD demo (JSON output + pipes)
6. Architecture note (how it discovers capabilities)
7. TSOK Lab origin / open source pledge
8. CTA: GitHub, Discord, X

---

### A3 Copy: "TSOK AI Lab: Why We Build Open Source Tools"

**Tags**: openSource, aiLab, developerTools, tsok

**Key points to hit**:
- What TSOK stands for (The Source of Knowledge)
- Our thesis: the AI ecosystem grows faster than its tooling
- Our philosophy: build what you need, open source it
- Our commitment: MIT, free forever, no VC pressure
- mcp2cli as the first proof point
- What types of tools we're building next (vague but exciting)
- Call to follow, star, contribute

---

## Discord Communities

### Message Template (adapt for each server)

```
Hey everyone 👋

I'm from TSOK AI Lab. We just open-sourced a tool called mcp2cli that 
I think some of you will find useful.

It turns any MCP server into a native CLI — auto-discovers tools/resources 
from JSON Schema, generates typed --flags, supports --json output for scripts.

Zero config ad-hoc use:
  mcp2cli --url http://your-server/mcp ls

GitHub: github.com/mcp2cli/source-code

Happy to answer questions! We're also looking for feedback from MCP builders 
specifically — if you have an MCP server, we'd love to know if it works.
```

**Adapt for each server**:
- **Rust Discord**: Mention it's written in Rust, happy to discuss impl
- **AI builders Discord**: Lead with AI agent automation angle
- **DevOps Discord**: Lead with CI/CD testing angle

---

## LinkedIn

### Post Template

```
🔬 We just open-sourced mcp2cli at TSOK AI Lab.

Problem we solved: Every team building on MCP (Model Context Protocol) has 
to write a custom client just to test or automate their server. It's repetitive, 
fragile, and adds days of overhead.

mcp2cli turns any MCP server into a native CLI in under 60 seconds:
→ Auto-discovers capabilities from JSON Schema  
→ Typed --flags, help text, shell-friendly output
→ Designed for CI/CD, platform automation, and AI agent orchestration

One binary. Any server. Real commands.

We built this because we kept writing the same boilerplate. Now it's yours.

✅ Free  ✅ Open source  ✅ MIT license

→ GitHub: [link]
→ Docs: [link]

If you're a platform engineer, SRE, or AI infrastructure builder — 
I'd love to hear if this solves something for you.

#DevTools #OpenSource #AI #MCP #CLI #PlatformEngineering #TSOK
```

---

## Newsletter Pitches

### TLDR Tech

**Submission format** (submit via tldr.tech/submit):
```
Project name: mcp2cli
URL: github.com/mcp2cli/source-code
Description: Open-source Rust tool that turns any MCP (Model Context Protocol) 
server into a native CLI. Auto-discovers tools/resources → typed --flags from 
JSON Schema. Supports CI/CD, --json output, background jobs, event streaming. 
For MCP server authors, DevOps teams, and AI agent builders.
```

### Console.dev

**Category**: CLI Tools  
Submit at console.dev/tools/submit

### Changelog News

Submit at changelog.com/news/submit with a short pitch linking to the GitHub repo and highlighting the OSS angle.

---

## Email / Personal Outreach

### To developer influencers

```
Subject: mcp2cli — might be useful for your [channel/audience]

Hey [Name],

I follow your [YouTube/blog] — especially enjoyed [specific piece].

We just open-sourced mcp2cli at TSOK AI Lab: github.com/mcp2cli/source-code

Short version: it turns any MCP server into a native CLI with zero protocol 
knowledge. The 90-second demo is at [link].

I thought your audience might find it interesting because [specific reason 
relevant to their content — e.g., they cover Rust tools, or MCP servers, 
or AI agent tooling].

We're not asking for anything specific — just wanted to make sure you knew 
about it. Happy to give a walkthrough if you're curious.

[Name] @ TSOK AI Lab
```

### To MCP server repo authors

```
Subject: Quick suggestion: add mcp2cli to your README

Hey [Name],

I saw your MCP server [repo name] — looks great.

We just released a free, open-source CLI for testing MCP servers: 
github.com/mcp2cli/source-code

It lets developers point mcp2cli at any server and immediately use it 
as a CLI — no protocol code required. Might be a nice addition to your 
"getting started" or "testing" section.

Something like:
"Test with mcp2cli: `mcp2cli --stdio ./your-server ls`"

Happy to help you add this if it would be useful. Keep up the great work!

[Name] @ TSOK AI Lab
```
