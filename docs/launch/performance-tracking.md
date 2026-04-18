# mcp2cli — Performance Tracking

How we measure the launch campaign, what signals matter, how to collect them, and what actions to take based on results.

---

## Philosophy

Tracking exists to answer one question at a time:

> *"Is what we're doing working, and if not, what should we do instead?"*

Most vanity metrics (impressions, views, likes) are informative but not actionable. We prioritize **leading indicators** — signals that predict actual developer adoption — over raw reach numbers.

**What we ultimately care about**: Developers trying, using, and recommending mcp2cli.

---

## KPI Hierarchy

### Tier 1 — North Star (what we optimize everything for)

| KPI | Target (30 days) | How to measure |
|-----|-----------------|----------------|
| **GitHub stars** | 500+ | GitHub repo page; `gh api repos/mcp2cli/source-code` |
| **GitHub unique cloners** | 200+ | GitHub Insights → Traffic → Clones |
| **Real user issues / discussions** | 10+ | GitHub Issues, Discussions tab |

GitHub stars are an imperfect proxy for interest, but they are the single most visible signal of adoption momentum in the OSS community. They compound (stars → trending → more stars).

---

### Tier 2 — Acquisition (getting developers to the top of the funnel)

| KPI | Target (30 days) | How to measure |
|-----|-----------------|----------------|
| GitHub repo unique visitors | 5,000+ | GitHub Insights → Traffic |
| GitHub referrer traffic sources | Track top 5 | GitHub Insights → Referrers |
| Product Hunt upvotes | 200+ | PH post page |
| Hacker News points | 50+ | HN post page |
| YouTube total views | 3,000+ | YouTube Studio |
| YouTube subscribers gained | 200+ | YouTube Studio |
| X/Twitter total impressions | 100,000+ | X Analytics |
| Reddit post upvotes (combined) | 200+ | Reddit analytics |

---

### Tier 3 — Engagement (developers taking action)

| KPI | Target (30 days) | How to measure |
|-----|-----------------|----------------|
| GitHub forks | 50+ | GitHub repo |
| GitHub issues opened (real users) | 10+ | GitHub Issues |
| GitHub discussions replies | 20+ | GitHub Discussions |
| Discord server members | 100+ | Discord server settings |
| YouTube watch time per video | > 40% avg | YouTube Studio |
| Medium article reads | 2,000+ total | Medium stats |
| dev.to article reactions | 100+ total | dev.to analytics |

---

### Tier 4 — Retention & Advocacy (the compound effect)

| KPI | Target (30 days) | How to measure |
|-----|-----------------|----------------|
| External mentions (blogs, tweets, repos) | 3+ | Google Alerts, GitHub Search |
| MCP server repos adding mcp2cli to docs | 2+ | GitHub search: "mcp2cli" |
| Returning GitHub visitors | Track | GitHub Insights |
| Newsletter feature | 1+ | Inbox / confirm |

---

## Measurement Setup

### 1. GitHub Insights (built-in, free)

Navigate to: `github.com/mcp2cli/source-code` → Insights → Traffic

Collect daily:
- **Views**: unique visitors and total pageviews
- **Clones**: unique and total
- **Referrers**: top traffic sources
- **Popular content**: which files/pages attract visitors

> Note: GitHub only retains 14 days of traffic data. **Export weekly** to a spreadsheet.

```bash
# Using GitHub CLI to export traffic data
gh api repos/mcp2cli/source-code/traffic/views  
gh api repos/mcp2cli/source-code/traffic/clones
gh api repos/mcp2cli/source-code/traffic/referrers
gh api repos/mcp2cli/source-code/traffic/popular/paths
```

Export to CSV weekly:
```bash
#!/bin/bash
# scripts/export-github-traffic.sh
DATE=$(date +%Y-%m-%d)
gh api repos/mcp2cli/source-code/traffic/views > "data/github-views-$DATE.json"
gh api repos/mcp2cli/source-code/traffic/clones > "data/github-clones-$DATE.json"
gh api repos/mcp2cli/source-code/traffic/referrers > "data/github-referrers-$DATE.json"
echo "Exported on $DATE"
```

---

### 2. UTM Parameters (link tracking)

Every external link to the GitHub repo or docs uses UTM parameters. This tells us which channel drives actual traffic to the repo.

**Standard UTM structure**:
```
utm_source    = channel (e.g. hacker-news, twitter, youtube, product-hunt, medium, reddit-rust)
utm_medium    = content type (social, video, article, email, forum)
utm_campaign  = launch-month1
utm_content   = specific asset ID (e.g. s1-launch-thread, v1-5min-intro, a1-native-cli)
```

**Example links**:
```
# Hacker News Show HN post
https://github.com/mcp2cli/source-code?utm_source=hacker-news&utm_medium=forum&utm_campaign=launch-month1&utm_content=show-hn-day0

# X launch thread
https://github.com/mcp2cli/source-code?utm_source=twitter&utm_medium=social&utm_campaign=launch-month1&utm_content=s1-launch-thread

# YouTube V1
https://github.com/mcp2cli/source-code?utm_source=youtube&utm_medium=video&utm_campaign=launch-month1&utm_content=v1-5min-intro

# Product Hunt
https://github.com/mcp2cli/source-code?utm_source=product-hunt&utm_medium=listing&utm_campaign=launch-month1&utm_content=ph-day0

# Reddit r/rust
https://github.com/mcp2cli/source-code?utm_source=reddit-rust&utm_medium=forum&utm_campaign=launch-month1&utm_content=reddit-rust-day0

# Medium A1
https://github.com/mcp2cli/source-code?utm_source=medium&utm_medium=article&utm_campaign=launch-month1&utm_content=a1-native-cli
```

GitHub's referrer report will show these UTM-tagged URLs. Parse them to attribute traffic to channels.

---

### 3. Self-Hosted Analytics (optional but recommended)

For more detailed click tracking on your own documentation site, add one of:

**Option A**: [Plausible Analytics](https://plausible.io) — privacy-first, no cookies, $9/month  
```html
<script defer data-domain="[your-docs-domain]" 
  src="https://plausible.io/js/script.js">
</script>
```

**Option B**: [Umami](https://umami.is) — open source, self-hostable, free
```bash
docker run -d -p 3000:3000 \
  -e DATABASE_URL=... \
  ghcr.io/umami-software/umami:postgresql-latest
```

Both give you page views, unique visitors, traffic sources, and custom events.

---

### 4. YouTube Studio

Access all analytics at studio.youtube.com:

| Metric | Where | What to watch |
|--------|-------|--------------|
| Views per video | Analytics → Overview | Total and per-video |
| Watch time / Avg view duration | Analytics → Engagement | Target > 40% |
| Click-through rate | Analytics → Reach | Target > 5% |
| Subscriber change | Analytics → Audience | Net gain |
| Traffic sources | Analytics → Reach → Sources | Identify best referrers |
| Top videos | Analytics → Overview | Which titles perform best |

**Key signal**: If watch time drops sharply at a specific timestamp, the content at that point is losing people. Use this to improve future videos.

---

### 5. X (Twitter) Analytics

Access at analytics.twitter.com or via the X app.

| Metric | What it tells you |
|--------|-----------------|
| Impressions | Reach of the tweet |
| Engagements | Likes, retweets, replies, clicks |
| Engagement rate | Engagements / Impressions — target > 2% for developer content |
| Link clicks | How many clicked through to GitHub |
| Profile visits | Awareness vs curiosity ratio |
| Follower growth | Sustained interest signal |

**Export**: Use twitter's data export (Settings → Your Account → Download archive) or tools like Tweetdeck analytics.

---

### 6. Google Alerts (External Mentions)

Set up alerts for:
```
"mcp2cli"
"mcp2cli" site:github.com
"mcp2cli" site:reddit.com
"TSOK AI Lab"
"mcp2cli"
```

At alerts.google.com. Delivers email when new content matches.

---

### 7. GitHub Search (Ecosystem Adoption)

Check weekly:
```
# Find external repos referencing mcp2cli
https://github.com/search?q=mcp2cli&type=code
https://github.com/search?q=mcp2cli&type=repositories
https://github.com/search?q=mcp2cli&type=issues
```

When an independent repo mentions mcp2cli in their README, it's an adoption signal that also drives organic traffic.

---

### 8. Telemetry (from the app itself)

mcp2cli includes built-in anonymous telemetry (opt-out). The local NDJSON file at `~/.local/share/mcp2cli/telemetry.ndjson` gives installation-level signals when users opt to share.

To aggregate (requires users to opt into shipping):

```bash
# Local analysis of your own usage (team dog-fooding)
cat ~/.local/share/mcp2cli/telemetry.ndjson | \
  jq -r '.event.command_category // .event.type' | \
  sort | uniq -c | sort -rn

# Error rate
cat ~/.local/share/mcp2cli/telemetry.ndjson | \
  jq -r 'select(.event.outcome=="error") | .event.command_category'
```

See [telemetry-collection.md](../telemetry-collection.md) for full backend setup.

---

## Daily Tracking Spreadsheet

Maintain a simple spreadsheet with one row per day:

### Columns

| Column | Source |
|--------|--------|
| Date | — |
| GitHub stars (total) | GitHub repo |
| GitHub stars (delta today) | Calculated |
| GitHub forks | GitHub repo |
| GitHub unique visitors (daily) | GitHub Insights |
| GitHub clones (unique) | GitHub Insights |
| Top referrer (today) | GitHub Insights |
| YouTube views (total) | YouTube Studio |
| YouTube subscribers | YouTube Studio |
| X impressions (today) | X Analytics |
| X followers | X Analytics |
| HN points (Day 0 only) | HN post |
| PH upvotes (Day 0 only) | PH post |
| Discord members | Discord server |
| Notes | Free text |

### Weekly Summary Template

Post this internally every Monday:

```markdown
## mcp2cli Launch — Week [N] Snapshot

**GitHub Stars**: [X] total (+[Y] this week) — [on track / behind / ahead]
**GitHub Visitors**: [X] unique this week
**Top Referrer**: [channel] with [N] visits
**YouTube Views**: [X] total, [N] this week
**X Impressions**: [X] this week
**Discord Members**: [X] total
**Issues from real users**: [X] new

**This week's wins**:
- 
-

**This week's surprises / learnings**:
-

**Next week plan**:
-
```

---

## Alert Thresholds — When to Change Strategy

| Signal | Threshold | Action |
|--------|-----------|--------|
| GitHub stars on Day 0 | < 20 in first 6 hours | Amplify HN / PH sharing; DM personal networks |
| HN Show HN < 5 points after 1h | Low traction | Post "Ask HN" version in Week 2 with different angle |
| YouTube V1 < 100 views in first week | Very low | Audit title/thumbnail; improve V2 SEO |
| X launch thread < 50 engagements | Low | Don't delete; post a shorter hook tweet linking to thread |
| Product Hunt rank < 20 by noon | Behind | Ask team members to share the PH link organically (never buy upvotes) |
| Week 1 stars < 100 by Day 7 | Below target | Activate newsletter submissions early; increase influencer outreach |
| Zero external mentions | Concerning | Direct outreach to 10 MCP ecosystem accounts manually |

---

## Month 1 Final Report Template

Publish this on Day 30 (both internally and as a public post):

```markdown
# mcp2cli Month 1 Launch Report

## Reach

| Channel | Result vs. Target |
|---------|------------------|
| GitHub stars | [X] / 500 target |
| GitHub forks | [X] / 50 target |
| GitHub unique visitors | [X] / 5,000 target |
| YouTube total views | [X] / 3,000 target |
| X total impressions | [X] / 100,000 target |
| Product Hunt upvotes | [X] / 200 target |
| Hacker News points | [X] / 50 target |
| Discord members | [X] / 100 target |

## Channel Performance (ranked by GitHub referral clicks)

1. [Channel] — [N] clicks → [N] clones
2. ...

## Top 3 Performing Assets

1. [Title] — [Key metric]
2. ...

## Unexpected Learnings

- 

## Month 2 Plan

→ [link to post-launch-roadmap.md]
```

---

## Tool Stack Summary

| Tool | Cost | Use |
|------|------|-----|
| GitHub Insights | Free | Traffic, clones, referrers |
| YouTube Studio | Free | Video analytics |
| X Analytics | Free | Social reach |
| Google Alerts | Free | External mentions |
| GitHub CLI | Free | Automated data export |
| Plausible / Umami | $9/mo or free (self-hosted) | Docs site analytics |
| mcp2cli telemetry | Free (built-in) | In-app usage signals |
| Spreadsheet (Google Sheets) | Free | Daily tracking |
