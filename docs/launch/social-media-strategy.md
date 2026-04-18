# mcp2cli — Social Media Strategy & Post Library
### X (Twitter) · LinkedIn · Short Video · Tracking

**Product**: mcp2cli  
**Lab**: TSOK — The Source of Knowledge AI Laboratory  
**Document version**: 1.0 — March 2026  
**Companion docs**: [launch-plan.md](launch-plan.md) · [content-calendar.md](content-calendar.md) · [channel-playbooks.md](channel-playbooks.md)

---

## How to Use This Document

1. **Strategy section** — read once, internalize the rules
2. **Post library** — 25 numbered, ready-to-publish posts across X and LinkedIn
3. **Video strategy** — short-form video formats, scripts, and production notes
4. **Tracking table** — paste into a spreadsheet, update after every post
5. **Post generation guide** — use the templates at the end to write *new* posts without starting from scratch

Every post has:
- A unique **Post ID** (P01–P25) for tracking
- Platform (X / LinkedIn / Both)
- Phase (Pre-launch / Launch / Post-launch)
- Content type (thread / single tweet / LinkedIn post / video post / engagement)
- The full copy — **copy-paste ready** with only `[LINKS]` to fill in
- A video companion note where one exists
- Target audience tag
- Linked article/asset reference where applicable

---

## Part 1 — Strategy

### 1.1 Platform Roles

These two platforms serve different but complementary functions in the launch:

| Platform | Role | Tone | Best content type | Primary audience |
|----------|------|------|-------------------|-----------------|
| **X (Twitter)** | Fast ignition, developer virality, real-time conversation | Sharp, direct, technical, a little punchy | Threads, GIFs, code snippets, single hooks | Rust devs, MCP builders, AI agent devs, CLI enthusiasts |
| **LinkedIn** | Professional credibility, platform/DevOps reach, TSOK lab brand | Thoughtful, narrative-led, outcome-focused | Long-form posts, stories, lab brand pieces | Platform engineers, DevOps leads, AI infrastructure teams, tech managers |

**Rule**: X punches first. LinkedIn reinforces and amplifies 24–48h later with a longer-form version of the same story.

---

### 1.2 Voice & Tone

#### TSOK AI Lab — brand voice

> *"We build the tools we wish existed. Then we open-source them."*

The lab voice is:
- **Practitioner-first** — we write like engineers who build things, not marketers who describe them
- **Honest** — we say what the tool does and doesn't do
- **Dry confidence** — not boastful, but not falsely modest either
- **No fluff** — no "excited to announce", no "thrilled to share", no "game-changer"

#### Good vs bad examples

| ❌ Avoid | ✅ Use instead |
|---------|--------------|
| "We are thrilled to announce..." | "We just shipped..." |
| "This game-changing tool..." | "This solves one specific problem:" |
| "Our revolutionary approach..." | "Here's how we built it:" |
| "Don't miss out on..." | "Try it in 60 seconds:" |
| "Synergizing AI capabilities..." | "Turn any MCP server into a CLI." |

---

### 1.3 Content Pillars

All 25 posts + future posts map to one of five pillars. This prevents the account from feeling like a one-note product pitch.

```
Pillar 1 — PROBLEM FRAMING     (show the pain before showing the fix)
Pillar 2 — PRODUCT DEMO        (show it working, not describe it)
Pillar 3 — TECHNICAL DEPTH     (for the engineers who want to know how)
Pillar 4 — USE CASE STORIES    (real workflows, real outcomes)
Pillar 5 — LAB & COMMUNITY     (TSOK identity, open source values, people)
```

**Target distribution across 25 posts:**
- Pillar 1 (Problem): 4 posts
- Pillar 2 (Demo): 8 posts
- Pillar 3 (Technical): 5 posts
- Pillar 4 (Use Cases): 5 posts
- Pillar 5 (Lab/Community): 3 posts

---

### 1.4 Posting Rhythm

#### Pre-launch (Days −7 to −1): Build anticipation
- **X**: 1 post every 2 days — problem framing, no product name yet
- **LinkedIn**: 1 post, Day −5 — the lab intro

#### Launch week (Days 0–7): Maximum frequency
- **X**: 1–2 posts per day — launch thread, videos, use cases, engagement
- **LinkedIn**: 1 post every 2 days — professional versions of key X content

#### Post-launch (Days 8–30): Sustained cadence
- **X**: 4–5 posts per week — tutorials, tips, community highlights, use cases
- **LinkedIn**: 2 posts per week — depth pieces, lab updates

#### Optimal posting times
| Platform | Best time (your timezone — adjust for audience) |
|----------|------------------------------------------------|
| X | 08:00–10:00 weekdays (developer morning scroll) |
| X | 13:00–15:00 (lunch scroll) |
| LinkedIn | 07:30–09:00 Tuesday–Thursday (commute/morning) |
| LinkedIn | 17:00–18:30 Tuesday–Thursday (end of work day) |

---

### 1.5 Short Video Strategy Overview

Short-form video lives primarily on:
- **X** — native video uploads (up to 2:20 native; longer as YouTube links)
- **LinkedIn** — native video uploads (up to 10 min; target 60–90 sec for feed)
- **YouTube Shorts** — 60 sec clips repurposed from longer videos

**Three video formats:**

| Format | Length | Style | Use on |
|--------|--------|-------|--------|
| **Terminal demos** | 30–90 sec | Screen recording, no voice, text captions | X, LinkedIn, Shorts |
| **Face-to-camera hooks** | 15–30 sec | Talking head intro, then demo | LinkedIn primarily |
| **Split-screen tutorials** | 60–120 sec | Code on left, terminal on right | X, YouTube Shorts |

**Production rules:**
- Always start with the demo, not the title card
- First 3 seconds must show something working — no intros
- Captions required (85% of LinkedIn video watched muted)
- Keep terminal font large enough to read on mobile (min 18pt)
- End every video with one action: "Link in bio" or "cargo install mcp2cli"

---

### 1.6 Hashtag Strategy

Use sparingly — developers despise hashtag spam. Max 3 tags per X post, max 5 per LinkedIn.

**Core tags** (use on almost every post):
```
#mcp2cli  #MCP  #OpenSource
```

**Rotation tags** (pick 1–2 relevant ones per post):
```
X:        #Rust  #CLI  #DevTools  #AItools  #DevOps  #TSOK
LinkedIn: #DeveloperTools  #PlatformEngineering  #AIInfrastructure  
          #RustLang  #TSOK  #OpenSourceAI  #DevOps  #SoftwareEngineering
```

---

## Part 2 — The Post Library (25 Posts)

> **Legend**  
> 🐦 = X only &nbsp;|&nbsp; 💼 = LinkedIn only &nbsp;|&nbsp; 🔁 = Both (different copy)  
> 🎬 = has video companion &nbsp;|&nbsp; 🔗 = references article/doc  
> **Phase**: PRE = pre-launch · DAY0 = launch day · W1 = week 1 · W2 = week 2 · W3–4 = weeks 3–4

---

### PRE-LAUNCH POSTS

---

#### P01 · 🐦 · PRE · Day −7 · Pillar 1 — Problem Framing
**Type**: Single tweet — anonymous problem hook  
**Audience**: MCP builders, DevOps  
**Video**: None  

```
Every time you build an MCP server, you write a test client.

Same boilerplate.
Same JSON-RPC setup.
Same session negotiation.

Over and over.

Something dropping next week that fixes this permanently.

🔔 Follow to see it first.
```

**Post notes**: No product name, no GitHub link. Pure problem resonance. Purpose is followers, not clicks.

---

#### P02 · 💼 · PRE · Day −5 · Pillar 5 — Lab Identity
**Type**: LinkedIn long-form intro  
**Audience**: Platform engineers, tech managers, AI infrastructure teams  
**Video**: None  
**Linked article**: [A3 — "TSOK AI Lab: Why We Build Open Source"](../articles/)

```
Three years ago, we started building AI infrastructure tools internally.

Every few months we'd solve a problem that we knew every team building 
on AI was also solving — and we kept thinking: why isn't this open?

Last year we formalized that instinct into TSOK AI Lab — The Source of 
Knowledge. A small lab with one mandate: build the developer tools the 
AI ecosystem needs, and give them away.

Today we're shipping our first public release.

It's a tool for anyone building with MCP (Model Context Protocol) — 
Anthropic's open standard for connecting AI models to tools and data.

The problem it solves: every MCP server needs a client to test it, 
call it, and automate it. Right now you write that client from scratch, 
every time.

We built a better way. Announcement tomorrow.

If you're a platform engineer, SRE, or AI infrastructure builder — 
follow along. This is the first of many tools from the lab.

🔬 TSOK — The Source of Knowledge AI Laboratory
```

**Post notes**: Publish Day −5. Sets up the lab narrative before the product drops. Tag 2–3 team members.

---

#### P03 · 🐦 · PRE · Day −3 · Pillar 2 — Demo  
**Type**: Single tweet with GIF  
**Audience**: All developers  
**Video**: 🎬 Demo GIF (30 sec terminal recording)  

```
Preview of what we're shipping Thursday.

Point it at any MCP server.
Watch what happens.

[GIF: mcp2cli --url http://server/mcp ls → capability table appears]

No config. No client code. Just commands.

github.com/mcp2cli/source-code (tomorrow)

#MCP #CLI #OpenSource
```

**GIF production note**: Record terminal at 2x speed. Show: (1) blank terminal → (2) one command → (3) capability table appearing. No mouse movements. White-on-black or dark theme only.

---

#### P04 · 🐦 · PRE · Day −1 · Pillar 1 — Problem  
**Type**: Single tweet — final teaser  
**Audience**: All  
**Video**: None  

```
Tomorrow.

Free. Open source. MIT.

For everyone who's ever had to write a MCP client just to test their server.

#mcp2cli
```

**Post notes**: One sentence max. Quiet confidence. Pin this tweet for 24 hours, then replace with the launch thread.

---

### LAUNCH DAY POSTS

---

#### P05 · 🐦 · DAY0 · Pillar 2 — Demo  
**Type**: Thread (8 tweets) — THE MAIN LAUNCH THREAD  
**Audience**: All developers  
**Video**: 🎬 Demo GIF in tweet 2  
**Linked article**: [A1 — "A native CLI for any MCP server"](../articles/)  

> *Full copy in [channel-playbooks.md](channel-playbooks.md#s1-main-launch-thread--day-0). Reproduced here for completeness.*

```
Tweet 1:
🧵 We just open-sourced mcp2cli at @tsok_lab.

Turn any MCP server into a native CLI in seconds: 
→ auto-discovered tools become typed --flags
→ resources become `get` commands  
→ no JSON-RPC, no protocol knowledge

Here's the 90-second demo, and why we built it 👇  1/8

---

Tweet 2:
The problem: every MCP server author writes a bespoke test client.
Every. Single. Time.

And once you've tested it — CI/CD wants to call it too.
And your AI agent loop wants to call it.
And the platform team wants a CLI for it.

Same plumbing, written over and over.  2/8

[Attach: demo GIF]

---

Tweet 3:
So we built mcp2cli.

Zero config — just point at a server:

  mcp2cli --url http://localhost:3001/mcp ls
  mcp2cli --url http://localhost:3001/mcp echo \
    --message "hello world"

Capabilities are auto-discovered.
Flags are typed from JSON Schema.
Results are human-readable or --json.  3/8

---

Tweet 4:
For permanent bindings, create a named config:

  mcp2cli config init --name work \
    --transport streamable_http \
    --endpoint http://server:3001/mcp

  mcp2cli link create --name work

Then it's just:

  work ls
  work echo --message hello
  work get file:///README.md

One binary. Any server. Real commands.  4/8

---

Tweet 5:
Pipes. Exit codes. --json output. --timeout. --background.

  work --json doctor | jq '.data.server.version'

  staging deploy --version 2.1.0 --background \
    && staging jobs watch

CI/CD. Automation. AI agents. It's a shell citizen.  5/8

---

Tweet 6:
We built this at TSOK — The Source of Knowledge AI Laboratory.

Our thesis: the AI ecosystem moves fast. Developer tooling lags.

mcp2cli is the first of many tools we're releasing to fix that.

Free. Open source. MIT. Forever.  6/8

---

Tweet 7:
What's under the hood:
→ Rust 🦀
→ MCP 2025-11-25 spec compliant
→ 135 tests
→ Streamable HTTP + Stdio transports
→ Daemon mode, background jobs, event streaming, OAuth
→ Full docs: 13 feature guides, 8 use-case articles

This isn't a prototype.  7/8

---

Tweet 8:
⭐ github.com/mcp2cli/source-code

📖 Docs: [DOCS LINK]
💬 Discord: [DISCORD LINK]
🎬 Video: [YOUTUBE V1 LINK]
🚀 Product Hunt: [PH LINK]

If you build MCP servers — test with this.
If you deploy AI tools — automate with this.

What will you wire up first? 👇  8/8

#mcp2cli #MCP #Rust #OpenSource #CLI
```

**Post notes**: Pin this thread immediately. Reply to every comment within 30 minutes on launch day. The final tweet is the one that gets retweeted — lead the CTA there.

---

#### P06 · 💼 · DAY0 · Pillar 2 — Demo  
**Type**: LinkedIn launch post  
**Audience**: Platform engineers, DevOps, AI infrastructure teams  
**Video**: 🎬 Demo GIF or 60-sec terminal video  
**Linked article**: [A1 — "A native CLI for any MCP server"](../articles/)  

```
We just open-sourced mcp2cli at TSOK AI Lab.

Here's the problem it solves in one sentence:

Every MCP server needs a client — and right now, you write that client 
from scratch, every single time.

mcp2cli ends that. Point it at any MCP server and it auto-discovers 
every tool, resource, and prompt, then generates a typed CLI from the 
JSON Schema:

  mcp2cli --url http://your-server/mcp ls
  mcp2cli --url http://your-server/mcp echo --message hello

For permanent configs:

  mcp2cli config init --name prod ...
  mcp2cli link create --name prod

  prod --json doctor | jq '.data.server'
  prod deploy --version 2.1.0 --background

Designed for real engineering workflows:
✅ --json output for scripts and CI/CD
✅ --timeout and --background for long operations
✅ Exit codes that pipelines can rely on
✅ Daemon mode for persistent connections
✅ Event streaming for monitoring

Build in Rust. 135 tests. MCP 2025-11-25 compliant. MIT license.

→ GitHub: github.com/mcp2cli/source-code
→ Full docs: [DOCS LINK]

This is TSOK Lab's first public release. More tools follow.
If you're building on MCP — try this. It takes 5 minutes.

#mcp2cli #MCP #OpenSource #DeveloperTools #PlatformEngineering
```

**Post notes**: Publish at 09:00 your time on launch day. Native video upload if possible (LinkedIn algorithm favors native). Tag team members.

---

#### P07 · 🐦 · DAY0 · Pillar 2 — Demo  
**Type**: Single tweet — video companion for V1  
**Audience**: All  
**Video**: 🎬 YouTube V1: "mcp2cli in 5 Minutes" (link + GIF thumbnail)  

```
Just published: mcp2cli in 5 minutes.

Install → discover → invoke.
No protocol knowledge.
No custom client code.

[YouTube V1 LINK]

#mcp2cli #Rust #MCP
```

**Post notes**: Post as a reply to P05 (thread tweet 8) AND as a standalone tweet. This double-posts the video for maximum reach.

---

### WEEK 1 POSTS

---

#### P08 · 🐦 · W1 · Day 1 · Pillar 5 — Community  
**Type**: Single tweet — social proof / reactions  
**Audience**: All  
**Video**: None  

```
24 hours in.

[X] stars. [Y] Product Hunt upvotes. [Z] Hacker News points.

Best comment so far:

"[PASTE BEST HN OR PH COMMENT]"

— thanks for the welcome. We're just getting started.

github.com/mcp2cli/source-code ⭐
```

**Post notes**: Screenshot the best HN or PH comment if it reads well. Fill in real numbers. This post performs well precisely because it's authentic.

---

#### P09 · 🐦 · W1 · Day 2 · Pillar 2 — Demo  
**Type**: Thread (4 tweets) — V2 video launch  
**Audience**: MCP server authors, DevOps engineers  
**Video**: 🎬 YouTube V2: "Testing MCP Servers — CI/CD Validation"  
**Linked article**: [docs/articles/e2e-conformance-testing.md](../articles/e2e-conformance-testing.md)  

```
Tweet 1:
New video: how to test any MCP server without writing a single line of client code.

Including GitHub Actions CI/CD config. Copy paste ready.

🎬 [YOUTUBE V2 LINK]  1/4

---

Tweet 2:
The three-command MCP test suite:

# 1. Health check
mcp2cli --url $MCP_URL doctor

# 2. Capabilities exist
mcp2cli --url $MCP_URL ls --json | \
  jq -e '.data.tools | map(.name) | contains(["echo"])'

# 3. Smoke test
mcp2cli --url $MCP_URL echo --json \
  --message "ci-test" | \
  jq -e '.data.content[0].text == "ci-test"'

Done.  2/4

---

Tweet 3:
This works because --json output gives you a structured envelope:

{
  "status": "success",
  "data": {
    "content": [{ "type": "text", "text": "ci-test" }]
  }
}

Pipeable. Scriptable. Exit codes on failure.
Same pattern as any Unix tool.  3/4

---

Tweet 4:
Full write-up on how we approach MCP server E2E testing:
[ARTICLE LINK — e2e-conformance-testing.md]

GitHub: github.com/mcp2cli/source-code

If you maintain an MCP server, let me know and I'll add you to the 
compatibility showcase.  4/4

#mcp2cli #MCP #CI #DevOps #OpenSource
```

---

#### P10 · 💼 · W1 · Day 3 · Pillar 4 — Use Case  
**Type**: LinkedIn long-form — "we replaced 500 lines"  
**Audience**: Platform engineers, tech leads  
**Video**: None  
**Linked article**: [A2 — "How We Replaced 500 Lines of MCP Client Code"](../articles/)  

```
We had a 500-line MCP test client.

We deleted it all.

It wasn't bad code. It was correct, tested, documented. 
But every MCP server we added to our stack meant writing and 
maintaining another one.

So we built mcp2cli — a single binary that auto-discovers any MCP 
server's capabilities from its JSON Schema and generates a typed CLI 
from them on the fly.

The test suite became:

  mcp2cli --url $MCP_URL doctor           # health
  mcp2cli --url $MCP_URL ls --json | jq   # capability check
  mcp2cli --url $MCP_URL echo --json \
    --message "smoke-test"                # smoke test

Three shell commands. No maintained client. No library dependency.

The full story (why we write custom clients, what breaks, and why the  
CLI model composites better into CI/CD):

→ [ARTICLE LINK]

GitHub: github.com/mcp2cli/source-code — free, MIT, open source.

#DeveloperTools #MCP #PlatformEngineering #OpenSource #TSOK
```

---

#### P11 · 🐦 · W1 · Day 3 · Pillar 3 — Technical Depth  
**Type**: Thread (5 tweets) — how capability discovery works  
**Audience**: Rust developers, protocol-curious engineers  
**Video**: None  
**Linked article**: [DESIGN-PROPOSAL.md](../../DESIGN-PROPOSAL.md)  

```
Tweet 1:
How does mcp2cli turn an MCP server's tools into typed CLI flags?

A technical thread on the discovery pipeline. 🧵  1/5

---

Tweet 2:
Step 1: Initialize an MCP session.

mcp2cli sends a standard MCP initialize request and negotiates capabilities 
with the server. This happens on every call unless daemon mode is active 
(warm connection pool).

The session reveals: protocol version, supported capabilities.  2/5

---

Tweet 3:
Step 2: Discover tools, resources, prompts.

mcp2cli calls tools/list, resources/list, prompts/list.

Each tool has a name, description, and an inputSchema (JSON Schema).

That inputSchema is the source of truth for the CLI flags.  3/5

---

Tweet 4:
Step 3: Build the CLI from the schema.

We use clap's runtime API — not proc macros — to dynamically construct 
a subcommand per tool, with each JSON Schema property becoming a typed 
--flag.

String → --flag <value>
Boolean → --flag (presence)
Number → --flag <n> (validated)
Object → --flag '{"json":"object"}'  4/5

---

Tweet 5:
Step 4: Cache the inventory.

Discovery adds latency. So on first run, the inventory is cached locally 
(in the state store). Subsequent calls use the cache and skip discovery.

`mcp2cli ls` invalidates and refreshes it.

Full architecture: [DESIGN-PROPOSAL LINK]  5/5

#Rust #MCP #CLI #OpenSource
```

---

#### P12 · 🐦 · W1 · Day 5 · Pillar 5 — Community  
**Type**: Single tweet — engagement / question  
**Audience**: All  
**Video**: None  

```
Question for everyone building on MCP:

What does your current test/dev workflow look like when you're 
iterating on an MCP server?

→ custom client code?
→ Claude / AI assistant?
→ manual JSON-RPC requests?
→ mcp2cli (if you've tried it)

Genuinely curious what the before state looks like. 👇

#MCP #DevTools
```

**Post notes**: This is a pure engagement post. Respond to every reply. Use the responses to inform future content.

---

#### P13 · 💼 · W1 · Day 4 · Pillar 5 — Lab Identity  
**Type**: LinkedIn — TSOK lab origin story  
**Audience**: Professional network, tech managers, AI teams  
**Video**: None  
**Linked article**: [A3 — "TSOK AI Lab: Why We Build Open Source"](../articles/)  

```
Why does TSOK AI Lab exist?

Honest answer: frustration.

For the past few years, we've been building AI-powered systems for real 
workflows. Every few months we'd hit the same pattern: spend days solving 
infrastructure problems that had nothing to do with our actual goal, 
because the tooling simply didn't exist.

And the tooling didn't exist because the AI ecosystem moves faster than 
anyone can commoditize it.

TSOK — The Source of Knowledge — is our answer to that.

We formalized a simple charter:
→ Build the tools we need
→ Build them properly (tested, documented, spec-compliant)
→ Open-source them with MIT license
→ Never extract rent from the ecosystem

mcp2cli is the first output. It turns any MCP server into a native CLI.
It solves a problem every MCP developer hits within their first hour.
It has 135 tests and full MCP 2025-11-25 spec compliance.
It is free. It will always be free.

This isn't charity. It's how we want to work.

If you're building on AI infrastructure and want tools that are actually 
engineered — follow along. We have more coming.

→ mcp2cli: github.com/mcp2cli/source-code
→ Full origin story: [ARTICLE LINK]

#TSOK #OpenSource #AIInfrastructure #DeveloperTools #OpenSourceAI
```

---

### WEEK 2 POSTS

---

#### P14 · 🐦 · W2 · Day 10 · Pillar 4 — Use Case  
**Type**: Thread (4 tweets) — AI agent use case  
**Audience**: AI developers, LLM builders, agent framework users  
**Video**: 🎬 YouTube V3: "mcp2cli + AI Agents"  
**Linked article**: [docs/articles/ai-agents-mcp-cli.md](../articles/ai-agents-mcp-cli.md)  

```
Tweet 1:
New video: calling MCP tools from an AI agent loop using shell commands.

No MCP client library. No protocol code. Just subprocess().

🎬 [YOUTUBE V3 LINK]  1/4

---

Tweet 2:
The pattern:

import subprocess, json

def call_tool(name: str, **kwargs) -> dict:
    args = ["work", "--json", name]
    for k, v in kwargs.items():
        args += [f"--{k}", str(v)]
    result = subprocess.run(args, capture_output=True)
    return json.loads(result.stdout)

# Now any MCP tool is just a function call:
result = call_tool("echo", message="hello from agent")  2/4

---

Tweet 3:
Background jobs work too — for long-running tasks:

# Kick off async
subprocess.run(["work", "--background", "deploy",
  "--version", "2.1.0"])

# Poll status
subprocess.run(["work", "--json", "jobs", "list"])

# Wait for completion  
subprocess.run(["work", "jobs", "wait", "--latest"])

Your agent doesn't block. The task runs in the server.  3/4

---

Tweet 4:
Full article on the AI agent + MCP CLI pattern:
[AI-AGENTS ARTICLE LINK]

GitHub: github.com/mcp2cli/source-code

#AI #MCP #LLM #AgentFramework #OpenSource #mcp2cli
```

---

#### P15 · 💼 · W2 · Day 14 · Pillar 4 — Use Case  
**Type**: LinkedIn — CI/CD use case, platform angle  
**Audience**: Platform engineers, DevOps leads, SREs  
**Video**: 🎬 Short clip from YouTube V2 (60 sec native upload)  
**Linked article**: [docs/articles/e2e-conformance-testing.md](../articles/e2e-conformance-testing.md)  

```
MCP servers in your CI pipeline — here's the pattern that works.

The problem most teams hit: they build an MCP server, ship it, 
and have no automated way to validate it in CI. Not real end-to-end 
validation — just "does it connect and return plausible output."

With mcp2cli, the entire test harness is:

  # In your GitHub Actions workflow:
  
  - name: Health check
    run: mcp2cli --url ${{ env.MCP_URL }} doctor
  
  - name: Verify capabilities
    run: |
      mcp2cli --url ${{ env.MCP_URL }} ls --json | \
      jq -e '.data.tools | map(.name) | 
             contains(["your-critical-tool"])'
  
  - name: Smoke test
    run: |
      mcp2cli --url ${{ env.MCP_URL }} your-critical-tool \
        --json --param value | \
        jq -e '.status == "success"'

No custom client. No test SDK. Three workflow steps.

The exit codes are shell-standard (0 = success, 1 = failure), 
so your pipeline gates work without any glue code.

We wrote up the full conformance testing approach here:
→ [ARTICLE LINK]

GitHub: github.com/mcp2cli/source-code — free, MIT, open source.

#CI #DevOps #PlatformEngineering #MCP #DeveloperTools #TSOK
```

---

#### P16 · 🐦 · W2 · Day 11 · Pillar 3 — Technical  
**Type**: Single tweet with code — tip format  
**Audience**: Rust devs, CLI power users  
**Video**: 🎬 30-sec terminal GIF showing the pattern  

```
Quick mcp2cli tip:

Named configs + symlinks turn your MCP server into a first-class binary:

  mcp2cli config init --name prod \
    --transport streamable_http \
    --endpoint https://prod.example.com/mcp
  
  mcp2cli link create --name prod

Now `prod` behaves like any installed CLI:
→ Tab completion works
→ --help is contextual  
→ Flags are typed from the server's JSON Schema

One binary. Unlimited server identities.

github.com/mcp2cli/source-code
```

**Post notes**: The GIF should show: (1) config init, (2) link create, (3) `prod --help` → server-shaped help text. This is the single most "wow" moment for developers seeing it the first time.

---

#### P17 · 🐦 · W2 · Day 13 · Pillar 1 — Problem  
**Type**: Thread (3 tweets) — problem framing for MCP server authors  
**Audience**: MCP server authors, SDK users  
**Video**: None  
**Linked article**: [A4 — "Building on MCP — Part 1: What is MCP?"](../articles/)  

```
Tweet 1:
The MCP ecosystem has a testing gap.

There's no standard client for MCP server authors to test with.
There's no equivalent of curl for MCP.

So everyone writes their own. And everyone writes the same thing.

This is the problem mcp2cli was built to solve.  1/3

---

Tweet 2:
What we've seen people use instead:
→ Claude desktop (UI, can't automate)
→ Manual JSON-RPC via Insomnia/Postman (tedious)
→ Custom Python/TypeScript client (requires maintenance)
→ The MCP Inspector (good for debug, not for CI)

None of these are "a CLI you can run in a pipeline."  2/3

---

Tweet 3:
mcp2cli is that CLI.

  mcp2cli --url http://localhost:3001/mcp ls
  mcp2cli --url http://localhost:3001/mcp echo --message test
  mcp2cli --url http://localhost:3001/mcp doctor

Zero config. Just works.

Does your MCP server pass the test?
→ github.com/mcp2cli/source-code

More on the MCP ecosystem: [ARTICLE LINK]  3/3

#MCP #OpenSource #DevTools #mcp2cli
```

---

### WEEK 3–4 POSTS

---

#### P18 · 🐦 · W3 · Day 15 · Pillar 4 — Use Case  
**Type**: Thread (4 tweets) — platform engineering use case  
**Audience**: SREs, platform teams, infrastructure engineers  
**Video**: 🎬 YouTube V4: "Platform Engineering with MCP"  
**Linked article**: [docs/articles/platform-engineering.md](../articles/platform-engineering.md)  

```
Tweet 1:
New video: platform engineering with MCP.

Deployment gates. K8s health checks. Infrastructure automation.
All via MCP server → mcp2cli.

🎬 [YOUTUBE V4 LINK]  1/4

---

Tweet 2:
The pattern that makes this work for platform teams:

# Multiple servers, multiple configs
mcp2cli config init --name dev-infra ...
mcp2cli config init --name prod-infra ...
mcp2cli link create --name dev-infra
mcp2cli link create --name prod-infra

# Now two separate CLIs pointing at different environments:
dev-infra get-pods --namespace api
prod-infra deploy --service api --version 2.1.0 --background  2/4

---

Tweet 3:
What makes this actually useful in production:

→ Event streaming: watch operations in real time
→ Daemon mode: keep connections warm, kill cold-start latency  
→ Background jobs: kick off long deploys, poll for completion
→ Exit codes: pipeline gates that don't need wrapper scripts  3/4

---

Tweet 4:
Article: platform engineering with mcp2cli
[PLATFORM-ENGINEERING ARTICLE LINK]

GitHub: github.com/mcp2cli/source-code

Who else is building MCP-backed infrastructure tooling? 👇  4/4

#SRE #PlatformEngineering #MCP #DevOps #OpenSource
```

---

#### P19 · 💼 · W3 · Day 16 · Pillar 5 — Lab  
**Type**: LinkedIn — lab origin story (narrative version)  
**Audience**: Professional network, potential contributors, tech managers  
**Video**: None  
**Linked article**: [A6 — "We Built mcp2cli at TSOK AI Lab — Here's Why"](../articles/)  

```
We built mcp2cli because we were tired of writing the same code.

Here's the honest story.

We kept building on MCP — Anthropic's open protocol for AI 
tool use. And every single time, within the first day:

"Right, I need a test client. Let me write that first."

It's 200–500 lines of boilerplate. Session negotiation, JSON-RPC 
plumbing, error handling, output formatting. You write it, it works, 
it does its job, and then it lives in a corner of the repo aging slowly.

When you add a second MCP server, you write it again slightly differently.

At TSOK AI Lab, we have a rule: if you've written the same thing three 
times, stop. Build it properly. Open-source it.

So we did.

mcp2cli is the result: a single binary that auto-discovers any MCP 
server's capabilities and generates a typed CLI from the JSON Schema. 
No custom client needed — ever.

One month since launch:
→ [X] GitHub stars
→ [N] MCP server repos added mcp2cli to their test instructions
→ [N] CI pipelines running mcp2cli validation

This is what the lab is about. Build what you need. Give it away.

Next tool is already in progress. Follow @tsok_lab if you want to see it.

Full story: [ARTICLE LINK]

#TSOK #OpenSource #DeveloperTools #AIInfrastructure
```

**Post notes**: Update the metrics in [square brackets] with real numbers when publishing. This post performs best 2+ weeks after launch when you have real data to show.

---

#### P20 · 🐦 · W3 · Day 19 · Pillar 2 — Demo  
**Type**: Thread (4 tweets) — build + test live demo  
**Audience**: MCP server authors, Rust devs  
**Video**: 🎬 YouTube V5: "Build & Test an MCP Server in 5 Minutes"  
**Linked article**: [docs/articles/local-dev-prototyping.md](../articles/local-dev-prototyping.md)  

```
Tweet 1:
New video: build an MCP server from scratch. Test every endpoint 
without writing a line of client code.

From zero to tested in under 15 minutes.

🎬 [YOUTUBE V5 LINK]  1/4

---

Tweet 2:
The workflow:

1. Scaffold server (Rust or TypeScript)
2. Start it: ./my-server or npx my-server
3. Point mcp2cli at it:
   mcp2cli --stdio "./my-server" ls
4. Call every tool as you build it:
   mcp2cli --stdio "./my-server" echo --message test
5. No test client to maintain. Ever.  2/4

---

Tweet 3:
The thing I didn't expect: having a real CLI available while building 
the server changes how you think about the API.

You feel the sharp edges immediately.
Flag names that are annoying to type.
Missing fields that should have defaults.
Error messages that don't help.

mcp2cli is the earliest possible user of your MCP server.  3/4

---

Tweet 4:
Local dev prototyping guide:
[LOCAL-DEV ARTICLE LINK]

GitHub: github.com/mcp2cli/source-code

What MCP server are you building? 👇  4/4

#Rust #MCP #DevTools #OpenSource #mcp2cli
```

---

#### P21 · 🐦 · W3 · Day 20 · Pillar 2 — Demo  
**Type**: Single tweet with GIF — mini-tutorial  
**Audience**: DevOps engineers, CLI users  
**Video**: 🎬 30-sec terminal GIF  

```
mcp2cli + jq is an underrated combo.

# List all tool names
work ls --json | jq -r '.data.tools[].name'

# Get full schema for one tool
work ls --json | jq '.data.tools[] | select(.name=="echo")'

# Count capabilities
work ls --json | jq '{
  tools: .data.tools | length,
  resources: .data.resources | length,
  prompts: .data.prompts | length
}'

One CLI. Full MCP surface. Fully composable.

github.com/mcp2cli/source-code
```

---

#### P22 · 💼 · W3–4 · Day 22 · Pillar 4 — Use Case  
**Type**: LinkedIn — multi-server / enterprise angle  
**Audience**: Platform teams, DevOps leads  
**Video**: None  
**Linked article**: [docs/articles/multi-server-workflows.md](../articles/multi-server-workflows.md)  

```
The pattern we see in mature MCP deployments:

One team. Multiple MCP servers. Multiple environments.
Each one needs a CLI. Each one gets tested in CI. Each one gets 
called by automation scripts.

Without mcp2cli, this looks like:
→ 3 custom clients (one per server)
→ Different conventions in each
→ CI pipelines with embedded protocol logic
→ Onboarding new engineers to 3 different "how to call the server" docs

With mcp2cli:

  mcp2cli config init --name billing ...
  mcp2cli config init --name notifications ...
  mcp2cli config init --name infrastructure ...

  mcp2cli link create --name billing
  mcp2cli link create --name notifications
  mcp2cli link create --name infrastructure

Now:
  billing get-invoice --id 12345
  notifications send --user alice --message "Invoice ready"
  infrastructure deploy --service billing --version 2.0

All follow the same CLI conventions.
All use the same --json pattern in CI.
All are discoverable via `billing ls`.

One tool. Any number of MCP servers. Consistent interface.

Multi-server patterns guide: [ARTICLE LINK]
GitHub: github.com/mcp2cli/source-code

#PlatformEngineering #DevOps #MCP #DeveloperTools #TSOK
```

---

#### P23 · 🐦 · W4 · Day 24 · Pillar 3 — Technical  
**Type**: Thread (4 tweets) — advanced features  
**Audience**: Power users, platform engineers  
**Video**: 🎬 YouTube V6: "Advanced — Daemon Mode, Background Jobs, Events"  

```
Tweet 1:
mcp2cli has features most users never discover.

New video covering the advanced layer: daemon mode, background jobs, 
event streaming.

🎬 [YOUTUBE V6 LINK]

A quick summary below. 🧵  1/4

---

Tweet 2:
Daemon mode — keep your MCP connection warm:

  mcp2cli daemon start

Now every subsequent command reuses the existing session.
Cold-start latency gone. Session negotiation: once.

Matters when you're making dozens of calls in a script 
or running tight automation loops.  2/4

---

Tweet 3:
Background jobs — kick off and come back:

  work deploy --version 2.1.0 --background
  # → returns immediately with a job ID
  
  work jobs list
  work jobs watch --latest
  # → streams status until done

Long-running operations don't block your terminal or pipeline.  3/4

---

Tweet 4:
Event streaming — observe in real time:

  # Stream to stderr (default)
  work deploy --version 2.1.0

  # POST each event as JSON to a webhook
  # (configured via events.http_endpoint in config)

  # Pipe to a Unix socket, SSE server, or shell command

Full video: [YOUTUBE V6 LINK]
Docs: [EVENTS FEATURE DOC LINK]  4/4

#mcp2cli #MCP #CLI #DevOps #Rust
```

---

#### P24 · 🐦 · W4 · Day 28 · Pillar 5 — Community  
**Type**: Single tweet — community challenge CTA  
**Audience**: All  
**Video**: None  

```
Show us what you've built with mcp2cli.

Any MCP server. Any workflow. Any use case you discovered 
that we didn't anticipate.

Drop it in the replies or in GitHub Discussions.

Best submission gets a feature in the docs.

#builtwithmcp2cli  #mcp2cli
```

**Post notes**: Start a parallel GitHub Discussion: "Show us your mcp2cli configs". Highlight the first 3 replies publicly.

---

#### P25 · 🔁 · W4 · Day 30 · Pillar 5 — Lab  
**Type**: Month 1 close — X thread + LinkedIn version  
**Audience**: All  
**Video**: None  

**X version (thread, 4 tweets):**
```
Tweet 1:
One month of mcp2cli.

Numbers:
→ [X] GitHub stars
→ [Y] forks
→ [Z] unique visitors
→ [N] MCP server repos using it as their test client

Thread on what worked, what surprised us, and what's next. 🧵  1/4

---

Tweet 2:
What worked:
→ The Show HN post: [X] points, quality comments, best referrer
→ Video V[N]: [metrics] — the [topic] angle resonated most
→ The CI/CD use case: most common thing people shared

What surprised us:
→ [something genuinely unexpected from user feedback]  2/4

---

Tweet 3:
What didn't work as expected:
→ [honest reflection — developers respect honesty]

What we learned:
→ [one concrete lesson]  3/4

---

Tweet 4:
Month 2 focus: [chosen fork from post-launch-roadmap]

Thank you for the stars, issues, DMs, and kind words.
You made launch week worth it.

⭐ github.com/mcp2cli/source-code
🔬 @tsok_lab  4/4
```

**LinkedIn version:**
```
One month since we launched mcp2cli at TSOK AI Lab.

[X] GitHub stars. [Y] forks. [Z] engineers testing their MCP 
servers with it instead of writing custom clients.

What I want to reflect on for one minute:

[2–3 sentence genuine reflection on the month — what you learned, 
one thing that surprised you, one thing that went better than expected]

The tool started as a solution to our own frustration. Seeing it 
solve the same frustration for others is why we build open-source.

Month 2 focus: [brief description of fork plan chosen].

Thank you to everyone who starred, filed issues, and told others.

→ github.com/mcp2cli/source-code
→ Follow @tsok_lab for what comes next

#TSOK #OpenSource #mcp2cli #MCP #DeveloperTools
```

---

## Part 3 — Short Video Strategy (Detailed)

### 3.1 Three Video Formats

#### Format FV1: Terminal Demo (30–90 seconds)
*Primary format for X. Repurpose as YouTube Shorts and LinkedIn video.*

**Production setup:**
```
Terminal: iTerm2 / Alacritty / Warp
Font: JetBrains Mono or Fira Code, 18pt minimum
Theme: Dark (Tokyo Night or Catppuccin Mocha)
Screen size: 1920×1080, crop to 16:9
Recording tool: OBS or asciinema → gif-encoder
Frame rate: 24fps minimum
```

**Scene structure (all under 90 sec):**
```
0:00–0:03  Start typing immediately. No title card. No intro.
0:03–0:30  Core demo: one clear action → visible result
0:30–0:60  Second action (the "and also" moment)
0:60–0:80  Outcome clearly visible on screen
0:80–0:90  Text overlay: "github.com/mcp2cli/source-code"
```

**Captions:** Required. Use burned-in captions for X (silent autoplay). Format: white text, dark background pill, bottom-center.

---

#### Format FV2: Face-to-Camera Hook (15–30 seconds)
*For LinkedIn primarily. Drives engagement via human presenter.*

**Script structure:**
```
[0–3 sec]   One sentence: the problem. Direct to camera. No warmup.
[3–10 sec]  One sentence: the solution. Bold claim.
[10–25 sec] Cut to terminal demo (screen recording)
[25–30 sec] Back to camera: single CTA. "Link in bio."
```

**LinkedIn-specific notes:**
- Record in portrait (9:16) if primarily LinkedIn/mobile
- First frame should show your face (increases autoplay CTR)
- Captions are not optional — LinkedIn default is muted

---

#### Format FV3: Split-Screen Tutorial (60–120 seconds)
*For YouTube Shorts and X. Higher production value.*

**Layout:**
```
LEFT 50%:   Code editor (VS Code, dark theme)
RIGHT 50%:  Terminal running mcp2cli
Bottom bar: Title text and progress indicator
```

**5 planned split-screen clips tied to the 25 posts:**

| Clip ID | Post | Topic | Duration |
|---------|------|-------|----------|
| FV3-01 | P09 | CI/CD 3 commands | 90 sec |
| FV3-02 | P11 | Discovery pipeline deep-dive | 120 sec |
| FV3-03 | P16 | Named config + symlink | 60 sec |
| FV3-04 | P20 | Build + test MCP server live | 120 sec |
| FV3-05 | P23 | Daemon mode + background jobs | 90 sec |

---

### 3.2 Video → Post Mapping

| Video | Duration | X post | LinkedIn post | YouTube | Shorts |
|-------|----------|--------|---------------|---------|--------|
| Demo GIF (launch) | 30 sec | P03, P05 | P06 | No | No |
| V1: 5 Min Intro | 5 min | P07 | P06 (clip) | Day 0 | Clip |
| V2: CI/CD Testing | 8 min | P09 | P15 (clip) | Day 2 | Clip |
| V3: AI Agents | 10 min | P14 | — | Day 10 | Clip |
| V4: Platform Eng | 12 min | P18 | P22 (clip) | Day 15 | Clip |
| V5: Build + Test | 15 min | P20 | — | Day 19 | Clip |
| V6: Advanced | 12 min | P23 | — | Day 24 | Clip |

---

### 3.3 Repurposing Rule

Every YouTube video generates:
1. One X thread (announcing it + 3-tweet summary)
2. One 60-sec clip for LinkedIn native video
3. One YouTube Short (best 60 seconds with Shorts-optimized vertical crop)
4. One GIF for use in future social posts

This means 7 long videos = 28+ social assets without creating new content.

---

## Part 4 — Tracking Table

Copy this table into a spreadsheet. One row per post. Update after publishing.

### Google Sheets setup suggestion

**Sheet 1: Post Tracker**

| Column | Content |
|--------|---------|
| Post ID | P01–P25 (and beyond) |
| Status | Draft / Scheduled / Published / Archived |
| Platform | X / LinkedIn / Both |
| Phase | Pre / Day0 / W1 / W2 / W3 / W4 / Post |
| Pillar | 1-Problem / 2-Demo / 3-Technical / 4-UseCase / 5-Lab |
| Publish date | Actual date published |
| Publish time | Actual time |
| Linked asset | Article URL, YouTube URL, or "none" |
| UTM content | e.g. `p05-launch-thread` |
| Impressions | Fill after 24h |
| Engagements | Likes + replies + shares/RT |
| Engagement rate | Engagements / Impressions |
| Link clicks | Clicks to GitHub or article |
| GitHub referrals | From GitHub Insights (fill weekly) |
| Notes | What worked, what to repeat |

**Sheet 2: Video Tracker**

| Column | Content |
|--------|---------|
| Video ID | V1–V6 + FV3-01 etc. |
| Title | |
| YouTube URL | |
| Publish date | |
| Views (Day 1) | |
| Views (Day 7) | |
| Views (Day 30) | |
| Avg view duration | |
| CTR (thumbnail) | |
| Subscribers gained | |
| Top traffic source | |
| Linked X post | |
| Linked LinkedIn post | |
| Shorts published | Yes/No |
| Shorts views | |

**Sheet 3: Channel Summary (Weekly)**

For each week, fill in:
- X: total impressions, total engagements, follower change, best post
- LinkedIn: total impressions, total engagements, follower change, best post
- YouTube: total views, subscribers gained, best video by views
- GitHub: stars delta, clones, top referrer

---

### KPI targets per post (guide for "is this working?")

| Platform | Post type | Minimum target | Good | Great |
|----------|-----------|----------------|------|-------|
| X | Single tweet | 500 impressions | 2,000 | 10,000+ |
| X | Thread | 2,000 impressions | 10,000 | 50,000+ |
| X | Tweet with GIF | 1,000 impressions | 5,000 | 20,000+ |
| LinkedIn | Standard post | 200 impressions | 1,000 | 5,000+ |
| LinkedIn | Post with video | 500 impressions | 2,000 | 10,000+ |
| LinkedIn | Long-form post | 300 impressions | 1,500 | 8,000+ |

**Engagement rate benchmarks (developer accounts):**
- X: 1–2% is normal; 3%+ is good; 5%+ is very strong
- LinkedIn: 2–3% is normal; 5%+ is good; 8%+ is very strong

---

## Part 5 — Post Generation Guide

Use this section to write *new* posts beyond P01–P25. Pick a template, fill in the variables, apply the voice guidelines, publish.

---

### Template T1 — Problem→Solution (X, single tweet)

```
[Relatable problem statement — one sentence, end with period not question mark]

[Brief amplification — why it's annoying]

[Solution: one sentence. Start with "mcp2cli" or the action verb.]

[Command or output that proves it]

github.com/mcp2cli/source-code
```

**Example:**
```
Debugging an MCP server with raw JSON-RPC is miserable.

Manual request construction. Session state to track.
One typo = cryptic error.

mcp2cli doctor shows you exactly what's wrong:

  work doctor

  ✓ Connection: ok
  ✓ Protocol version: 2025-11-25
  ✗ Tool "deploy": schema validation warning

github.com/mcp2cli/source-code
```

---

### Template T2 — Mini-tutorial thread (X, 3–5 tweets)

```
Tweet 1: [Hook: what this thread teaches] — [emoji] 1/N

Tweet 2: [First step or concept with code block] 2/N

Tweet 3: [Second step — the "and this is why it works" layer] 3/N

Tweet [N]: 
[Article or doc link for depth]
GitHub: github.com/mcp2cli/source-code
[Relevant hashtags]  N/N
```

---

### Template T3 — LinkedIn use case story

```
[One sentence describing a specific pain the audience has felt]

[3–5 sentences on the before state — be specific, be honest]

With mcp2cli:

[3–5 line command block showing the solution]

[2–3 sentences on the after state — what changed, what got better]

[One sentence on the broader implication]

→ [Article link if applicable]
GitHub: github.com/mcp2cli/source-code
[3–5 hashtags]
```

---

### Template T4 — Lab/TSOK brand post (LinkedIn)

```
[Open with a belief statement: what TSOK believes, not what TSOK sells]

[2–3 sentences on why the lab exists / an honest moment of origin]

[One concrete example: mcp2cli solving the specific problem]

[Forward-looking: what the lab is working on next, vague but genuine]

→ Follow @tsok_lab for upcoming releases.
[2–3 hashtags]
```

---

### Template T5 — Community / engagement (X)

```
[Question directed at the audience — not about the product, about their experience]

[2–3 answer options formatted as bullets or arrows]

[Genuine invitation: "Curious what the before state looks like" or similar]

[Optional: note that mcp2cli is an option if they've tried it]

#MCP #DevTools [one more]
```

---

### Post generation checklist

Before publishing any new post:

- [ ] Does it start with the reader's problem, not our product?
- [ ] Is it free of filler words ("excited to", "thrilled to", "game-changing")?
- [ ] If it contains code, is the code correct and actually runnable?
- [ ] If it contains a link, does the link have a UTM `utm_content` tag?
- [ ] Does the LinkedIn version have captions on any video?
- [ ] Is there one clear CTA — and only one?
- [ ] Is the TSOK lab credited somewhere (at minimum `@tsok_lab` mention)?

---

## Appendix — Quick Reference

### Post ID quick lookup

| ID | Phase | Platform | Topic |
|----|-------|----------|-------|
| P01 | Pre | X | Anonymous problem hook |
| P02 | Pre | LinkedIn | TSOK lab intro |
| P03 | Pre | X | Demo GIF teaser |
| P04 | Pre | X | Final teaser |
| P05 | Day 0 | X | Main launch thread (8 tweets) |
| P06 | Day 0 | LinkedIn | Launch post |
| P07 | Day 0 | X | V1 video share |
| P08 | W1 D1 | X | 24h social proof |
| P09 | W1 D2 | X | V2 + CI/CD thread |
| P10 | W1 D3 | LinkedIn | "We replaced 500 lines" |
| P11 | W1 D3 | X | How discovery works (technical) |
| P12 | W1 D5 | X | Community question |
| P13 | W1 D4 | LinkedIn | TSOK lab origin story |
| P14 | W2 D10 | X | V3 + AI agents thread |
| P15 | W2 D14 | LinkedIn | CI/CD platform angle |
| P16 | W2 D11 | X | Named config tip + GIF |
| P17 | W2 D13 | X | MCP testing gap thread |
| P18 | W3 D15 | X | V4 + platform engineering |
| P19 | W3 D16 | LinkedIn | Lab origin narrative (with numbers) |
| P20 | W3 D19 | X | V5 + build + test thread |
| P21 | W3 D20 | X | mcp2cli + jq GIF tip |
| P22 | W3–4 D22 | LinkedIn | Multi-server patterns |
| P23 | W4 D24 | X | V6 advanced features thread |
| P24 | W4 D28 | X | Community challenge |
| P25 | W4 D30 | Both | Month 1 close |

### UTM content IDs (for all 25 posts)

```
P01 → p01-problem-hook
P02 → p02-tsok-intro-li
P03 → p03-demo-gif-teaser
P04 → p04-final-teaser
P05 → p05-launch-thread
P06 → p06-launch-li
P07 → p07-v1-video
P08 → p08-24h-recap
P09 → p09-v2-cicd
P10 → p10-500lines-li
P11 → p11-discovery-technical
P12 → p12-community-q
P13 → p13-tsok-story-li
P14 → p14-v3-agents
P15 → p15-cicd-li
P16 → p16-named-config-tip
P17 → p17-mcp-testing-gap
P18 → p18-v4-platform
P19 → p19-lab-numbers-li
P20 → p20-v5-build-test
P21 → p21-jq-tip
P22 → p22-multiserver-li
P23 → p23-v6-advanced
P24 → p24-community-challenge
P25 → p25-month1-close
```

Full UTM format:
```
?utm_source=[twitter|linkedin]&utm_medium=social&utm_campaign=launch-month1&utm_content=[ID above]
```
