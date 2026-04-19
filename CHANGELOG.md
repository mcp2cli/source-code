## 0.1.1 (2026-04-19)

### Features

- **release:** Nx release + multi-arch binary workflow + SHA256 install.sh
- **site:** add Astro-powered landing page + docs site for GitHub Pages
- **site:** switch to mcp2cli.dev custom domain
- **site:** fix terminal, add copy buttons + mermaid diagrams
- **site:** zsh-style code blocks, reorder landing, simplify hero CTAs
- **site:** one-command installer + subtle light-mode code blocks
- **site:** streamline header/footer
- **site:** render landing terminal through `<Code>` so it matches code blocks exactly
- **site:** MCP favicon, dark-by-default theme, brighter dark code palette
- **site:** bash-highlighted terminal + brighter dark-mode code palette
- **site:** add Demo header above the landing terminal
- **site:** vesper code theme, cool-slate light-mode code surface
- **site:** add INSTALL.md + SKILL.md to footer, relabel LLMs.txt
- **site:** replace cargo install fallback with AI-agent prompt
- **site:** interactive CLI demo on the landing (mpp.dev pattern)
- **site:** theme-aware interactive terminal + install section polish
- **site:** section anchors + rename demo eyebrow
- **site:** monochrome code-block theme matches interactive terminal
- **site:** unify code-block chrome between docs and interactive terminal
- **site:** restore zsh-style colour palette + highlight interactive terminal commands
- **site:** wizard stays as scrollback after a pick, TUI-style
- **site:** always tail the terminal + favicon as header logo
- **site:** switch dark code theme from vesper to one-dark-pro
- **site:** one-light theme + reliable terminal auto-tail
- **site:** turn protocol coverage into a card grid with direction arrows
- **site:** script the interactive terminal to cycle through fresh picks
- **site:** TSOK attribution sentence in footer
- **site:** emit OTLP/HTTP spans for page_view, link_click, code_copied
- **telemetry:** wire real HTTP shipping to telemetry.mcp2cli.dev/ingest
- **telemetry:** OTLP/HTTP JSON + web→install→first-run attribution
- **telemetry:** decouple CLI telemetry from website / installer

### Fixes

- **ci:** pnpm version-conflict + formatting drift
- **site:** drop duplicate H1 on every docs page
- **site:** stop interactive terminal from scrolling the page when the wizard appears
- **site:** align interactive-terminal palette to Shiki's bash colours
- **site:** finish aligning interactive-terminal palette with Shiki
- **site:** dark-mode code blocks actually render dark colours
- **site:** give the MCP-spec icon visible hover feedback
- **site:** add vertical gap between adjacent code blocks
- **site:** anchor targets no longer hidden behind sticky header
- **site:** resolve relative .md and repo-source links in docs
- **site:** give the 'Copied!' tooltip readable foreground

### Refactors

- **site:** smooth terminal auto-tail

### Docs

- improve source comments, remove internal docs, rewrite protocol coverage
- label bare code fences with inferred languages
- expose all three install flows on Getting Started + Install
- **install:** lead with the curl install.sh one-liner

### ❤️ Thank You

- Andrii Tsok