# mcp2cli — 30-Day Launch Plan

**Product**: mcp2cli  
**Lab**: TSOK — The Source of Knowledge AI Laboratory  
**Target date**: [SET YOUR LAUNCH DATE]  
**License**: Open source (MIT), free forever  
**Goal**: Maximum developer awareness and adoption in the MCP/AI tools ecosystem

---

## Executive Summary

mcp2cli is a developer tool in a fast-growing market: the MCP (Model Context Protocol) ecosystem is expanding rapidly as AI assistants gain tool-use capability. Every MCP server author needs to test their server. Every DevOps engineer wants to orchestrate AI tools from CI/CD. Every platform team wants to wrap internal services.

We launch once, we launch loud, and we let the tool sell itself — because the 30-second demo is impossible to ignore.

---

## Target Audiences

### Primary (reach in month 1)

| Audience | Size Estimate | Why They Care |
|----------|--------------|---------------|
| MCP server authors | ~5,000 on GitHub | They need a test client — we give them one for free |
| Rust developers | ~4M worldwide | Built in Rust, speaks their language |
| DevOps / platform engineers | ~15M worldwide | CLI automation for AI tools |
| CLI tool enthusiasts | Large OSS subcommunity | Always looking for elegant tools |
| AI agent developers | ~500K+ | CLI-to-LLM integration pattern |

### Secondary (reach in months 2+)

| Audience | Channel |
|----------|---------|
| TypeScript/Node MCP devs | npm community, dev.to |
| Python MCP devs | Reddit r/Python, PyPI |
| Enterprise platform teams | LinkedIn, case studies |
| AI startup engineers | Hacker News, tech blogs |

---

## Goals for Month 1

### Hard targets

| Metric | Target |
|--------|--------|
| GitHub stars | 500+ |
| GitHub forks | 50+ |
| Unique visitors to repo | 5,000+ |
| Product Hunt upvotes | 200+ |
| Hacker News Show HN points | 50+ |
| YouTube video views (total) | 3,000+ |
| X/Twitter total impressions | 100,000+ |
| Medium/dev.to article reads | 2,000+ |
| Discord/community joins | 100+ |

### Soft targets

- At least 3 unsolicited GitHub issues from real users trying the tool
- At least 1 external blog post or tweet about the tool from someone not on the team
- At least 2 MCP server repos adding mcp2cli to their README as the suggested test client

---

## Channel Strategy

### Tier 1 — Maximum Impact

These channels drive the bulk of initial attention:

| Channel | Peak Day | Why |
|---------|----------|-----|
| **Product Hunt** | Launch Day | Developer audience, discovery engine, permanent social proof |
| **Hacker News Show HN** | Launch Day | Reaches senior devs, opinionated engineers, high signal |
| **X (Twitter)** | Pre-launch + Launch week | Viral thread potential; AI/dev community very active |
| **YouTube** | Week 1–2 | Tutorial videos rank in search; long-tail discovery for months |

### Tier 2 — Sustained Reach

| Channel | Timing | Why |
|---------|--------|-----|
| **Reddit** (r/rust, r/commandline, r/programming, r/LocalLLaMA) | Week 1–2 | Deep engagement, technical crowd |
| **dev.to** | Week 1–3 | Developer-native, SEO friendly, syndicatable |
| **Medium** | Week 1–3 | Broader reach, long-form storytelling |
| **GitHub** | Pre-launch + ongoing | Stars, discussions, good README = organic discovery |

### Tier 3 — Community & Long Tail

| Channel | Timing | Why |
|---------|--------|-----|
| **LinkedIn** | Week 2–4 | Platform/DevOps engineers; TSOK lab brand |
| **Discord** (Rust, MCP, AI servers) | Week 1–4 | Direct community engagement |
| **Newsletters** (TLDR Dev, Changelog, etc.) | Week 2–3 | High-trust, curated reach |
| **Podcast pitches** | Month 2+ | Long lead time; plan now |

---

## Pre-Launch Preparation (2 Weeks Before)

### Infrastructure

- [ ] **GitHub repo** — public, polished README, clear install instructions, LICENSE, CONTRIBUTING, issue templates
- [ ] **GitHub Discussions** — enable with seeded questions
- [ ] **Documentation site** — all docs published and linked from README
- [ ] **Demo GIF / video** — 60-second terminal recording showing the "wow moment"
- [ ] **Social accounts** — @mcp2cli or @tsok_lab on X confirmed
- [ ] **Discord server** — `#mcp2cli` channel or standalone server

### Content Pre-Production

Create all content 1 week before launch so launch day is execution, not creation:

- [ ] **YouTube Video 1** — "The 5-Minute Introduction" (fully edited, ready to publish)
- [ ] **YouTube Video 2** — "Testing MCP Servers with mcp2cli" (fully edited, ready to publish)
- [ ] **X launch thread** — written, reviewed, screenshots embedded, ready to post
- [ ] **Product Hunt submission** — drafted: tagline, description, media/screenshots, first comment
- [ ] **Hacker News Show HN** post — written and reviewed
- [ ] **Blog post 1** — "mcp2cli: A native CLI for any MCP server" (on Medium and dev.to)
- [ ] **Reddit posts** — written per subreddit (each slightly different)
- [ ] **Community messages** — Discord copy ready for each server

### Social Warm-Up (Week −1)

Don't launch cold. Build a tiny audience before the launch day:

- Post 3–5 "teaser" tweets from the @tsok_lab account:
  - Day −7: "We've been building something for everyone who builds MCP servers. Coming next week."
  - Day −5: Share the problem: "Every MCP server needs a CLI. Writing one every time is insane. We fixed that."
  - Day −3: Share the GIF/demo: "Preview: any MCP server → native CLI in seconds."
  - Day −1: "Tomorrow. Open source. Free. #mcp2cli"
- Post GitHub repo (private beta) link to 5–10 friendly developers for early stars

---

## Launch Day (Day 0) — Hour-by-Hour

### 00:01 PST — Product Hunt goes live

Product Hunt resets at midnight PST. Submit the night before for 00:01 publish:

**Action checklist:**
- [ ] Product goes live at https://www.producthunt.com/posts/mcp2cli
- [ ] Team members upvote immediately (do NOT ask for upvotes in the post itself)
- [ ] Post the first comment as the maker: the origin story + a welcoming call-to-action
- [ ] Share the PH link in all personal networks

### 09:00 PST — Hacker News Show HN

HN peaks at ~9am PST. Post exactly:

```
Show HN: mcp2cli – Turn any MCP server into a native command-line app
```

**Action checklist:**
- [ ] Submit Show HN post
- [ ] Team ready to respond to comments within 5 minutes
- [ ] Share HN link internally — upvote from personal accounts (authentic engagement only)

### 09:15 PST — X launch thread

Fire the main X thread immediately after HN goes up:

```
🧵 We just open-sourced mcp2cli — turn any MCP server into a native CLI. 

No JSON-RPC. No protocol knowledge. Just commands.

Here's what it does and why we built it at @tsok_lab 👇

1/8
```

**Action checklist:**
- [ ] Post full thread (see [channel-playbooks.md](channel-playbooks.md) for full copy)
- [ ] Include demo GIF in tweet 2
- [ ] Final tweet: GitHub link + Product Hunt link

### 10:00 PST — Publish YouTube Video 1

- [ ] Publish "mcp2cli in 5 Minutes" to YouTube
- [ ] Cross-post link to all active channels

### 12:00 PST — Reddit wave

Post to all subreddits simultaneously (one post each, different framing):

- [ ] r/rust
- [ ] r/commandline
- [ ] r/selfhosted (if applicable)
- [ ] r/LocalLLaMA

### 14:00 PST — Blog post publishes

- [ ] "mcp2cli: A native CLI for any MCP server" goes live on Medium + dev.to
- [ ] Share links across all channels

### All day — Engagement

- Monitor and respond to every HN comment
- Monitor and respond to every PH comment
- Respond to all X replies
- Watch GitHub issues/discussions and respond within 2 hours

---

## Week 1 (Days 1–7) — Content Wave

### Day 1 (Tuesday) — Debrief + second wave

- Post "Top comments from yesterday's launch" thread on X (social proof)
- Share HN thread screenshot if it peaked
- Engage any dev bloggers/influencers who mentioned it

### Day 2 (Wednesday) — YouTube Video 2

**Publish**: "Testing MCP Servers with mcp2cli — Zero Code CI/CD Validation"

- Post announcement thread on X
- Share to Reddit r/mcp (if exists), r/devops, r/MachineLearning
- Add to dev.to post as embedded

### Day 3 (Thursday) — Deep dive article

**Publish**: Medium/dev.to — "How We Replaced 500 Lines of MCP Client Code with One Command"

- Framing: a problem story → solution story
- Include the architecture diagram
- End with a CTA to GitHub

### Day 4 (Friday) — TSOK lab origin post

**Publish**: Medium/LinkedIn — "TSOK AI Lab: Why We Build Open Source Tools"

- This is the lab's story: who we are, what we build, why it's open
- Position mcp2cli as the first of many tools from TSOK
- Link to the lab's website/repo

### Day 5 (Saturday) — Community engagement

- Post a "What would you build with mcp2cli?" thread on X
- Create a GitHub Discussion: "Show us your mcp2cli configs"
- Reach out to 3–5 MCP server repos on GitHub with a friendly PR adding mcp2cli to their docs

### Day 6–7 (Weekend) — Monitor & respond

- Engage all comments, issues, discussions
- Document any unexpected use cases that emerged
- Screenshot the best quotes/reactions for Week 2 social proof

---

## Week 2 (Days 8–14) — Influencer & Community Push

### Influencer outreach (Days 8–9)

Identify and reach out to developers with audiences in the target space:

**Tier 1 — Direct outreach (DM/email):**
- Rust ecosystem YouTubers / Twitch streamers
- MCP ecosystem builders (check GitHub for popular MCP server repos)
- AI tools newsletter authors (TLDR, Changelog, etc.)
- Developer advocates at Anthropic, Cursor, Zed, or other AI tool companies

**Outreach template:**
```
Hey [Name],

I saw your [content] about [MCP/Rust/AI tools]. We just open-sourced mcp2cli 
— a tool that turns any MCP server into a native CLI in seconds.

It might be interesting to your audience because [specific reason].

GitHub: github.com/mcp2cli/source-code
Demo GIF: [link]

Happy to chat or give a tour if you're curious.

[Team]
```

### YouTube Video 3 (Day 10)

**Publish**: "mcp2cli + AI Agents: Calling Any MCP Tool from Shell Scripts"

- This hits the AI agent developer audience
- Shows the `--json` output connecting to Python/bash AI agent loops
- High sharability in LLM/agent developer community

### Community Discord blitz (Days 11–12)

Post in:
- Rust official Discord `#showcase`
- Rust community Discord
- Anthropic developer Discord (if available)
- Any MCP-specific Discord servers
- AI tools communities

Keep it concise: 2–3 sentences + GitHub link + one-liner demo.

### Dev.to "series" launch (Day 13)

Start a structured series on dev.to:

**Series: "Building on MCP"**
- Part 1: "What is MCP and why does it matter?" (educational, not product)
- Part 2: "Testing MCP Servers Without Writing Client Code" (tool showcase)
- Part 3: "MCP in CI/CD Pipelines" (advanced use case)

The educational first article builds trust before the pitch.

### LinkedIn post (Day 14)

**Audience**: Platform engineers, DevOps, AI infrastructure teams

```
We just open-sourced mcp2cli at TSOK AI Lab.

Problem: The MCP ecosystem is growing fast, but every team building on 
MCP has to write a custom client just to test their server.

mcp2cli turns any MCP server into a typed CLI in under 60 seconds.

We built this because we were tired of writing the same plumbing over and over.
Now it's your tool too. Free, open source, MIT.

→ [GitHub link]
#DevTools #OpenSource #AI #CLI #MCP
```

---

## Week 3 (Days 15–21) — Depth & Tutorials

### YouTube Video 4 (Day 15)

**Publish**: "mcp2cli for Platform Engineers — Infrastructure Automation with MCP"

- Targets the DevOps/SRE audience
- Shows the platform engineering article examples in practice
- Links to the platform-engineering.md doc

### Medium "origin story" post (Day 16)

**Title**: "We Built mcp2cli at TSOK AI Lab — Here's Why"

This is the founders' narrative:
- What problem triggered the build
- What TSOK AI Lab is and why it exists
- What philosophy guides the tools we build
- What comes next

This humanizes the project and builds lab brand.

### Newsletter pitch wave (Days 17–18)

Submit to developer newsletters:

| Newsletter | Audience | Submission |
|------------|----------|------------|
| TLDR Tech | 750k+ developers | tldr.tech/submit |
| Console.dev (tools) | Dev tools enthusiasts | console.dev/tools |
| Changelog weekly | OSS community | changelog.com/news/submit |
| Bytes.dev | JavaScript devs | bytes.dev |
| Cooperpress newsletters | Multiple dev audiences | cooperpress.com |
| Hacker Newsletter | HN weekly digest | hackernewsletter.com |
| StatusCode Weekly | Web developers | statuscode.com |

**Submission copy (keep it short):**
```
mcp2cli — turns any MCP server into a native CLI. Auto-discovers tools/resources 
→ typed --flags from JSON Schema. One binary, any server. Built at TSOK AI Lab.
→ github.com/mcp2cli/source-code
```

### YouTube Video 5 (Day 19)

**Publish**: "Local Development with mcp2cli — Test Your MCP Server Without Writing Client Code"

- Targets the MCP server authors — the most motivated users
- Hands-on coding session: building a simple MCP server and testing with mcp2cli
- "Build a toy MCP server in Rust and validate it in 5 minutes"

### X "use case" thread series (Days 20–21)

Three separate threads, each one a mini-tutorial:

**Thread A**: "How to build a CI/CD pipeline that validates your MCP server in 3 commands"

**Thread B**: "Turn your Kubernetes MCP server into `k8s get-pods` — in 60 seconds"

**Thread C**: "How mcp2cli caught a bug in our MCP server that our unit tests missed"

---

## Week 4 (Days 22–30) — Community & Momentum

### Community roundup post (Day 22)

**X thread / blog post**: "30 days of mcp2cli — what worked, what surprised us"

- Screenshots of GitHub stars graph
- Best user quotes / issues / discussions
- Use cases we didn't anticipate
- What's coming next

This is social proof + momentum signal.

### YouTube Video 6 (Day 24)

**Publish**: "mcp2cli Advanced: Daemon Mode, Background Jobs, and Event Streaming"

- Advanced features for power users
- Keeps the audience that already tried the basics engaged
- Generates "hidden features" discussion

### "Built with TSOK" post (Day 25)

**Platform**: Medium + LinkedIn

Position this as a lab summary:
- mcp2cli is TSOK Lab's first public tool release
- Our philosophy: build the tools we need, open-source them
- What the lab is working on next (tease without over-promising)

### Community challenges (Days 26–28)

Launch a "Show Us What You Built" challenge:
- GitHub Discussion + X hashtag `#builtwithmcp2cli`
- Prize: feature your project in official docs examples
- Highlight 3 submissions in a dedicated thread

### Month 1 close post (Day 30)

**All channels** — A clean summary post:

```
30 days. [X] stars. [Y] contributors. [Z] countries.

mcp2cli is just getting started.

Here's what Month 2 looks like → [link to roadmap]
```

---

## Content Asset Summary

### Video Content (6 videos in month 1)

| # | Title | Target Audience | Publish Day |
|---|-------|----------------|-------------|
| V1 | "mcp2cli in 5 Minutes" | All developers | Day 0 |
| V2 | "Testing MCP Servers — Zero Code CI/CD" | MCP server authors, DevOps | Day 2 |
| V3 | "mcp2cli + AI Agents: Shell-Based Tool Use" | AI/agent developers | Day 10 |
| V4 | "Platform Engineering with MCP" | SRE/DevOps | Day 15 |
| V5 | "Build & Test an MCP Server in 5 Minutes" | MCP builders | Day 19 |
| V6 | "Advanced: Daemon Mode, Jobs, Events" | Power users | Day 24 |

### Written Content (9 articles)

| # | Title | Platform | Day |
|---|-------|----------|-----|
| A1 | "mcp2cli: A native CLI for any MCP server" | Medium + dev.to | Day 0 |
| A2 | "How We Replaced 500 Lines of MCP Client Code with One Command" | Medium | Day 3 |
| A3 | "TSOK AI Lab: Why We Build Open Source Tools" | Medium + LinkedIn | Day 4 |
| A4 | "Building on MCP — Part 1: What is MCP?" | dev.to (series) | Day 13 |
| A5 | "Building on MCP — Part 2: Testing Without Client Code" | dev.to (series) | Day 14 |
| A6 | "We Built mcp2cli at TSOK AI Lab — Here's Why" | Medium | Day 16 |
| A7 | "Building on MCP — Part 3: MCP in CI/CD" | dev.to (series) | Day 17 |
| A8 | "30 Days of mcp2cli — What Worked" | Medium | Day 22 |
| A9 | "Built with TSOK — Our First Open Source Release" | Medium + LinkedIn | Day 25 |

### Social Content (X / Twitter)

| # | Type | Day |
|---|------|-----|
| S1 | Main launch thread (8 tweets) | Day 0 |
| S2 | "Top reactions to yesterday's launch" | Day 1 |
| S3 | Video 2 announcement thread | Day 2 |
| S4 | "What would you build with mcp2cli?" engagement thread | Day 5 |
| S5 | Tutorial mini-thread: CI/CD in 3 commands | Day 20 |
| S6 | Tutorial mini-thread: K8s in 60 seconds | Day 21 |
| S7 | Tutorial mini-thread: Bug we caught with mcp2cli | Day 21 |
| S8 | 30-day retrospective thread | Day 30 |

### Platform submissions

| Platform | Day | Action |
|----------|-----|--------|
| Product Hunt | Day 0 | Launch |
| Hacker News Show HN | Day 0 | Post |
| Reddit r/rust | Day 0 | Post |
| Reddit r/commandline | Day 0 | Post |
| Reddit r/LocalLLaMA | Day 0 | Post |
| Reddit r/devops | Day 2 | Post |
| Discord channels (5+) | Days 11–12 | Post |
| Newsletter submissions (8) | Days 17–18 | Submit |
| Console.dev tools | Day 1 | Submit |

---

## TSOK Lab Positioning

Every piece of content should consistently include these elements:

### Mention in all posts
> *mcp2cli is built at [TSOK — The Source of Knowledge AI Laboratory](https://tsok.org), an open-source AI tooling lab dedicated to building the developer tools the AI ecosystem needs.*

### Lab Brand Points
- **Open by default** — all TSOK tools are MIT licensed, free forever
- **Practitioner-built** — tools made by engineers, for engineers, from real pain
- **Quality over speed** — mcp2cli has 135 tests and full MCP spec compliance
- **Community first** — GitHub Discussions, responsive maintainers, no VC pressure

### Lab CTA for all content
```
⭐️ Star mcp2cli on GitHub: github.com/mcp2cli/source-code
🔬 Follow TSOK AI Lab: @tsok_lab
💬 Join the community: [Discord link]
```

---

## Budget Estimates

This launch plan is designed to run with **zero paid advertising**. All channels are organic.

| Activity | Cost | Notes |
|----------|------|-------|
| GitHub repo hosting | $0 | Free |
| YouTube | $0 | Free |
| Product Hunt | $0 | Free listing |
| Hacker News | $0 | Free |
| dev.to / Medium | $0 | Free accounts |
| Newsletter submissions | $0 | Most free; some paid ($50–200) |
| Discord server | $0 | Free |
| Domain for demo site (optional) | $10–15 | Optional |
| Screen recording software | $0–100 | OBS is free; Cleanshot ~$29 |
| **Total** | **$0–315** | **Essentially free** |

Optional paid amplification (Month 2):
- Carbon Ads (developer-native advertising): $200–500/month
- Reddit promoted posts on r/rust: $100–300

---

## Risk Mitigation

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| HN Show HN gets no traction | Medium | Have a second HN post angle ready for Week 2; "Ask HN" format |
| Product Hunt gets buried | Medium | Choose a Tuesday–Thursday launch; avoid Mondays and Fridays |
| Low engagement Week 1 | Low-Medium | Pre-seeded community; influencer DMs from Day 1 |
| Competitor announces similar tool | Low | Focus on TSOK lab narrative and spec completeness |
| Crash/bug discovered after launch | Low-Medium | Have a hotfix plan; respond transparently in all channels |
| Content creation falls behind | Medium | All content pre-produced before Day 0; no improvised posts |

---

## Team Roles

| Role | Responsibilities |
|------|----------------|
| **Launch lead** | Single decision-maker; approves all content; monitors all channels |
| **Content creator** | Videos, articles, social copy — pre-produced by Day −3 |
| **Community manager** | Responds to all comments/issues within 2h on launch day |
| **Developer advocate** | Influencer outreach, Discord community, GitHub issue responses |
| **Metrics owner** | Daily report: stars, traffic, HN/PH position, video views |

---

## See Also

- [content-calendar.md](content-calendar.md) — day-by-day calendar
- [channel-playbooks.md](channel-playbooks.md) — per-channel execution scripts
- [performance-tracking.md](performance-tracking.md) — metrics and dashboards
- [post-launch-roadmap.md](post-launch-roadmap.md) — Month 2+ fork plans
