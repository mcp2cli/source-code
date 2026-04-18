# mcp2cli site

Astro-powered documentation + landing page for mcp2cli. Deployed to
GitHub Pages on every push to `main` that touches `site/**` or
`docs/**`. Live at `https://mcp2cli.dev/`.

## Stack

- [Astro 5](https://astro.build) (SSG, zero-JS by default).
- [Tailwind CSS 3](https://tailwindcss.com) + `@tailwindcss/typography`
  for prose.
- React islands for anything interactive (currently: just the theme
  toggle is inline; more islands land as needed).
- MDX + Shiki for syntax-highlighted code blocks.
- `@astrojs/sitemap` for sitemap generation.

## Layout

```
site/
├── astro.config.mjs
├── tailwind.config.mjs
├── package.json
├── public/                 static assets (favicon, robots.txt)
└── src/
    ├── content.config.ts   Astro content collection reading ../docs
    ├── layouts/            BaseLayout + DocsLayout
    ├── components/         Header, Footer, Terminal, FeatureGrid, …
    ├── pages/
    │   ├── index.astro     landing page
    │   ├── docs/[...slug].astro
    │   ├── llms.txt.ts     raw endpoint: ../docs/files/llms.txt
    │   ├── llms-full.txt.ts
    │   └── SKILL.md.ts     raw endpoint: ../docs/files/SKILL.md
    ├── styles/globals.css
    └── lib/                helpers (cn, nav builder)
```

The docs content collection points at `../docs/**/*.md`, so any edit
to a repository doc is picked up on the next build. `AGENTS.md` is
excluded (contributor-internal), `SKILL.md` is served as a raw agent
skill file at `/SKILL.md`, and `llms.txt` / `llms-full.txt` are
served at their canonical root paths per the [llms.txt
convention](https://llmstxt.org).

## Local development

```bash
cd site
pnpm install
pnpm dev            # http://localhost:4321/
pnpm build          # → dist/
pnpm preview        # serve the built output
```

## Deploying

The `.github/workflows/pages.yml` workflow builds the site and deploys
via the official `actions/deploy-pages` action. Before the first run,
enable GitHub Pages for the repo:

1. **Settings → Pages → Build and deployment → Source: GitHub Actions**.

Subsequent pushes to `main` that touch `site/**` or `docs/**` deploy
automatically.

## Custom domain

The site is served from `mcp2cli.dev` via GitHub Pages. The domain is
pinned by:

- `public/CNAME` (contents: `mcp2cli.dev`) — copied into the built
  artifact so the custom-domain setting persists across deploys.
- `astro.config.mjs` — `site: 'https://mcp2cli.dev'`, `base: '/'`.

If the custom domain ever needs to be retired, remove `public/CNAME`,
flip `base` back to `'/<repo>'`, and update the Pages settings.
