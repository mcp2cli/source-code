# mcp2cli — Post-Launch Roadmap (Month 2+)

High-level fork plans for what to do after the initial month. These are directional, not committed — choose based on what Month 1 data tells you.

---

## Decision Framework

At the end of Month 1, the data will point toward one or more of these shapes:

| Pattern | What it means | Best fork plan |
|---------|--------------|----------------|
| High stars, low issues | People are watching but not using | → [Fork A: Adoption Push](#fork-a-adoption-push) |
| High issues, good engagement | Active users, rough edges | → [Fork B: Quality & Docs](#fork-b-quality-and-docs) |
| 1–2 unexpected use cases dominating | Discovered a bigger audience than expected | → [Fork C: Pivot Messaging](#fork-c-pivot-messaging) |
| Influencers or communities going big | Organic momentum | → [Fork D: Community Flywheel](#fork-d-community-flywheel) |
| Slow start but strong dev interest | Good product, weak reach | → [Fork E: Paid & Partnership Amplification](#fork-e-paid--partnership-amplification) |
| Strong traction everywhere | Things are working well | → [Fork F: Scale & Ecosystem Expansion](#fork-f-scale--ecosystem-expansion) |

Choose one primary fork and pick elements from others — don't run all paths simultaneously.

---

## Fork A: Adoption Push

**Signal**: Stars and awareness are solid, but actual installs and real usage (GitHub clones, telemetry events, issues) are low.

**Problem to solve**: People found the project interesting but didn't take the leap to try it.

**Hypothesis**: The friction of installation or initial setup is blocking adoption.

### Actions (Month 2–3)

**Reduce friction aggressively**:
- [ ] Publish a pre-built binary release workflow (GitHub Actions → attach binaries to releases)
- [ ] Add a one-liner install script: `curl -fsSL https://install.mcp2cli.dev | sh`
- [ ] Add Homebrew tap: `brew install mcp2cli/tap/mcp2cli`
- [ ] Add Cargo install as the default path (already works, improve the UX messaging)
- [ ] Improve the first-run experience: interactive setup wizard for new users

**Content**: Lower the barrier with "5 minutes to first command" content:
- [ ] Short-form video series: "mcp2cli quick tips" (60-second X/YouTube Shorts)
- [ ] Interactive playground on docs site using WebAssembly or a hosted demo server
- [ ] GIF gallery of common use cases on the README

**Distribution**:
- [ ] Submit to awesome-rust, awesome-mcp lists
- [ ] Submit to toolbox.computer, devhunt.org, alternativeto.net
- [ ] Reach out to VS Code extension authors in the MCP space

**Success target by Month 4**: 200+ unique monthly cloners; telemetry showing multi-day usage from same installation IDs.

---

## Fork B: Quality and Docs

**Signal**: Active users are engaged but GitHub issues show friction, confusion, or missing functionality.

**Problem to solve**: The tool works but the experience has rough edges. Power users are engaged; newcomers struggle.

**Hypothesis**: Investing in DX, error messages, and documentation yields the highest retention improvement.

### Actions (Month 2–3)

**DX improvements**:
- [ ] Audit all error messages — every error should suggest next steps
- [ ] Add a guided `mcp2cli init` interactive setup wizard
- [ ] Improve `mcp2cli doctor` output to diagnose all common connection problems
- [ ] Add `mcp2cli config lint` to validate config files before use

**Documentation**:
- [ ] "Cookbook" section: 20 copy-paste recipes for common use cases
- [ ] Troubleshooting guide: top 10 error states + solutions
- [ ] Video series: "mcp2cli explained" — one feature per 5-min video
- [ ] Screencasts for each of the 8 use-case articles

**Testing & reliability**:
- [ ] Expand integration test suite to cover error edge cases
- [ ] Add test against 5 popular real-world MCP servers (catalog compatibility)
- [ ] Publish compatibility matrix: "Tested with these MCP servers"

**Success target**: GitHub issue resolution time < 48h; user-reported friction drops (fewer "how do I...?" issues).

---

## Fork C: Pivot Messaging

**Signal**: Month 1 data shows one use case vastly outperforming others. E.g. the AI agent posts went viral, or the CI/CD content drove most clones, or the Rust community is the biggest audience.

**Problem to solve**: We launched with broad messaging, but the data shows a specific niche is responding much stronger. We're leaving adoption on the table by staying broad.

**Hypothesis**: Re-focusing the brand message on the dominant use case will accelerate adoption among that audience.

### Potential pivots based on data

**Scenario: AI agents is the dominant use case**
- Rename/rebrand messaging: "The CLI interface for your AI agent's MCP tools"
- Partnerships with LangChain, LangGraph, AutoGen, Crew AI
- Publish integrations: mcp2cli adapter patterns for each major agent framework
- Target: AI agent developers on r/LocalLLaMA, Hugging Face community

**Scenario: MCP server testing is dominant**
- Positioning: "The official-ish CLI testing client for MCP servers"
- Reach out to the MCP spec authors / Anthropic for possible endorsement mention
- Publish: "mcp2cli MCP conformance test suite" as a standalone artifact
- Target: Every MCP SDK repository

**Scenario: DevOps/CI is dominant**
- Positioning: "mcp2cli: DevOps-native MCP automation"
- Integration guides: GitHub Actions, GitLab CI, CircleCI, ArgoCD
- Publish: mcp2cli GitHub Action (marketplace)
- Target: DevOps newsletters, DevOps-focused YouTube channels

**Success target**: 2x conversion rate from visits to clones in the focused segment.

---

## Fork D: Community Flywheel

**Signal**: Strong organic social sharing, external blog posts appearing, developers building on top of mcp2cli, or unsolicited use cases.

**Problem to solve**: There's momentum, but it's not organized. The community is diffuse — capturing and amplifying it will compound adoption.

**Hypothesis**: A well-run community turns users into advocates, advocates into contributors, and contributors into co-marketers.

### Actions (Month 2–3)

**Community infrastructure**:
- [ ] Discord — structured, with dedicated channels: #help, #showcase, #mcp-servers, #contribute
- [ ] GitHub Discussions — curated weekly "thread of the week"
- [ ] Monthly community call (30 min): demo + Q&A + "what's next" preview
- [ ] GitHub Sponsors page — even at $0, shows commitment signal

**Recognition programs**:
- [ ] "Featured projects" section in README and docs
- [ ] "mcp2cli contributor" Discord badge
- [ ] Highlight community-built MCP server + mcp2cli combos in monthly blog post

**Contributor enablement**:
- [ ] "Good first issue" label on 10+ GitHub issues
- [ ] Architecture guide for contributors: "How the dispatch system works"
- [ ] Test contribution guide: how to add integration tests for new MCP servers

**Content co-creation**:
- [ ] Guest blog post program: community members write about their use cases
- [ ] "Build with us" stream: Twitch/YouTube live coding session with community

**TSOK Lab brand amplification**:
- [ ] TSOK Lab blog: regular posts about the tools and the thinking behind them
- [ ] TSOK Lab newsletter: monthly digest of what the lab is working on
- [ ] Introduce the next TSOK tool in preview — keeps the lab brand growing

**Success target by Month 4**: 10+ external contributors; 5+ unsolicited blog posts; community-generated content outpacing team content.

---

## Fork E: Paid & Partnership Amplification

**Signal**: Strong product-market fit (good engagement, low churn signals) but slower-than-desired reach growth. The organic channels have been maximized.

**Problem to solve**: The tool is good and users love it, but reach is limited without broader distribution.

**Hypothesis**: Small, targeted paid placements in high-quality developer channels will unlock the next adoption cohort.

### Actions (Month 2–3)

**Paid advertising (developer-native)**:

| Channel | Budget | Format |
|---------|--------|--------|
| Carbon Ads | $200–400/mo | Text ad on developer sites |
| Reddit Promoted Posts (r/rust, r/devops) | $100–300 | Native post format |
| TLDR Newsletter (sponsored intro) | $500–1000 | Short ad in TLDR Tech email |
| Console.dev sponsorship | Variable | Developer tool spotlight |

**Partnership targets**:

| Partner | Why | Approach |
|---------|-----|----------|
| Anthropic developer relations | They built MCP — mcp2cli helps their ecosystem | DM / PR outreach |
| Zed editor team | MCP-forward editor, tech-savvy audience | GitHub issue or DM |
| Cursor editor team | Heavy MCP users | Same |
| Popular MCP server repos (top 20 by stars) | Natural fit in their test docs | Friendly PR or outreach |
| Rust Foundation | OSS Rust tool — possible feature in Rust newsletter | Email |

**Conference targeting**:
- Submit talk proposals: RustConf, FOSDEM, All Things Open, local DevOps meetups
- Demo at local developer meetups

**Success target**: 1 paid channel with > 2x ROI (measured in GitHub clones vs. cost); 2+ partnership features.

---

## Fork F: Scale & Ecosystem Expansion

**Signal**: Everything is working. Month 1 targets exceeded. Community is growing. Tool is getting real use.

**Problem to solve**: How do we sustain momentum and expand the ecosystem to make mcp2cli the *default* tool in the MCP space?

**Hypothesis**: The MCP ecosystem is growing rapidly. If mcp2cli becomes the standard test/automation client for MCP servers, every new MCP server that ships is a potential user.

### Actions (Month 2–6)

**Ecosystem integration**:
- [ ] mcp2cli GitHub Action (marketplace) — `uses: mcp2cli/source-code-action@v1`
- [ ] VS Code extension: run mcp2cli commands from the editor
- [ ] mcp2cli Docker image: `docker run tsokorg/mcp2cli --url ...`
- [ ] mcp2cli in Homebrew core (not just tap)
- [ ] Nix flake / AUR package (Linux packaging)

**Specification alignment**:
- [ ] Maintain public MCP spec compliance table (unique differentiator)
- [ ] Contribute to MCP specification discussions / GitHub
- [ ] "mcp2cli conformance badge" for server repos that pass all tests

**SDK/Library split**:
- [ ] Extract core MCP client logic as `mcp-client-rs` crate on crates.io
- [ ] This enables Rust developers to embed the client in their own tools
- [ ] mcp2cli becomes the reference implementation of the library

**Enterprise features (open source tier)**:
- [ ] Multi-server config management (profiles across many servers)
- [ ] Audit logging mode
- [ ] Policy-based capability filtering
- [ ] SSO integration documentation

**TSOK Lab product expansion**:
- [ ] Release next TSOK tool (builds on mcp2cli learnings)
- [ ] TSOK product page as a hub for all lab tools
- [ ] "TSOK toolbelt" concept: a suite of AI infrastructure tools

**Content at scale**:
- [ ] Monthly "State of MCP" blog post — positions TSOK as ecosystem thought leader
- [ ] Quarterly release video: new features, community highlights
- [ ] Partner with MCP server authors on joint tutorial content

**Success target by Month 6**:
- 2,000+ GitHub stars
- mcp2cli referenced in 50+ external repos
- mcp2cli GitHub Action in active use
- 1 major conference talk accepted
- TSOK lab brand recognized in the MCP/AI tools ecosystem

---

## TSOK Lab Brand — Long-Term Vision

Regardless of which fork path is chosen, these lab-level goals run in parallel:

### 3-month lab goals
- [ ] TSOK AI Lab has a dedicated website with the lab's mission and tools
- [ ] mcp2cli is featured as "Lab Tool #1"
- [ ] Blog posts establish TSOK as a technical voice in the AI tools space
- [ ] 500+ followers on @tsok_lab X account

### 6-month lab goals
- [ ] Second TSOK tool announced
- [ ] TSOK lab newsletter running monthly (500+ subscribers)
- [ ] TSOK invites community contributions to future tools

### 12-month lab vision
- [ ] TSOK is recognized as "the lab that builds the tools AI developers need"
- [ ] Multiple open-source tools under the TSOK umbrella
- [ ] Community of developers following TSOK for new releases
- [ ] TSOK tools cited in industry articles and talks

---

## Content Fork Plans (Month 2)

Continuing the content engine, regardless of product fork:

### Video series options

| Series | Episodes | Best if... |
|--------|----------|-----------|
| "Build on MCP" — hands-on coding | 6–8 | Fork B or D; Rust/MCP audience |
| "AI Tools in the Terminal" | 4–6 | Fork C (AI agent pivot) |
| "Platform Engineering with MCP" | 4 | Fork E/F (enterprise reach) |
| "TSOK Lab: How We Build" | 3 | Any; builds lab brand |
| "mcp2cli advanced masterclass" | 3 livestreams | Fork D (community flywheel) |

### Article series options

| Series | Posts | Best if... |
|--------|-------|-----------|
| "MCP Server Reviews" — test popular servers | Weekly | Any; drives SEO + ecosystem |
| "State of MCP" monthly report | Monthly | Fork F (thought leadership) |
| "Building at TSOK" lab journal | Monthly | Any; builds lab narrative |
| "mcp2cli user stories" | Monthly | Fork D (community) |

---

## Decision Checklist at Day 30

Use the Month 1 final report to answer:

1. Which channel drove the most GitHub clones? _(focus there in Month 2)_
2. What are users trying to do that isn't working? _(Fork B)_
3. Has any one use case dominated unexpectedly? _(Fork C)_
4. Are developers sharing or building on the tool without prompting? _(Fork D)_
5. Is the organic ceiling approaching? _(Fork E)_
6. Are Month 1 metrics significantly above target? _(Fork F)_

Pick one primary fork. Execute for 60 days. Reassess.
