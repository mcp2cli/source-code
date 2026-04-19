## 0.1.1 (2026-04-19)

### Features

- **release:** Nx release + multi-arch binary workflow + SHA256 install.sh ([5727d68](https://github.com/mcp2cli/source-code/commit/5727d68))
- **site:** add Astro-powered landing page + docs site for GitHub Pages ([a4714a8](https://github.com/mcp2cli/source-code/commit/a4714a8))
- **site:** switch to mcp2cli.dev custom domain ([4ba239a](https://github.com/mcp2cli/source-code/commit/4ba239a))
- **site:** fix terminal, add copy buttons + mermaid diagrams ([072366c](https://github.com/mcp2cli/source-code/commit/072366c))
- **site:** zsh-style code blocks, reorder landing, simplify hero CTAs ([#0](https://github.com/mcp2cli/source-code/issues/0), [#141414](https://github.com/mcp2cli/source-code/issues/141414))
- **site:** one-command installer + subtle light-mode code blocks ([#0](https://github.com/mcp2cli/source-code/issues/0), [#141414](https://github.com/mcp2cli/source-code/issues/141414), [#1](https://github.com/mcp2cli/source-code/issues/1), [#768390](https://github.com/mcp2cli/source-code/issues/768390), [#8](https://github.com/mcp2cli/source-code/issues/8))
- **site:** streamline header/footer ([90910ec](https://github.com/mcp2cli/source-code/commit/90910ec))
- **site:** render landing terminal through <Code> so it matches code blocks exactly ([0b397d9](https://github.com/mcp2cli/source-code/commit/0b397d9))
- **site:** MCP favicon, dark-by-default theme, brighter dark code palette ([#0](https://github.com/mcp2cli/source-code/issues/0))
- **site:** bash-highlighted terminal + brighter dark-mode code palette ([3945295](https://github.com/mcp2cli/source-code/commit/3945295))
- **site:** add Demo header above the landing terminal ([d0a7406](https://github.com/mcp2cli/source-code/commit/d0a7406))
- **site:** vesper code theme, cool-slate light-mode code surface ([#0](https://github.com/mcp2cli/source-code/issues/0))
- **site:** add INSTALL.md + SKILL.md to footer, relabel LLMs.txt ([6853240](https://github.com/mcp2cli/source-code/commit/6853240))
- **site:** replace cargo install fallback with AI-agent prompt ([cab9929](https://github.com/mcp2cli/source-code/commit/cab9929))
- **site:** interactive CLI demo on the landing (mpp.dev pattern) ([544bf4e](https://github.com/mcp2cli/source-code/commit/544bf4e))
- **site:** theme-aware interactive terminal + install section polish ([#24292](https://github.com/mcp2cli/source-code/issues/24292), [#6](https://github.com/mcp2cli/source-code/issues/6), [#7](https://github.com/mcp2cli/source-code/issues/7), [#22863](https://github.com/mcp2cli/source-code/issues/22863), [#4](https://github.com/mcp2cli/source-code/issues/4), [#0](https://github.com/mcp2cli/source-code/issues/0))
- **site:** section anchors + rename demo eyebrow ([baa004e](https://github.com/mcp2cli/source-code/commit/baa004e))
- **site:** monochrome code-block theme matches interactive terminal ([#24292](https://github.com/mcp2cli/source-code/issues/24292), [#6](https://github.com/mcp2cli/source-code/issues/6), [#7](https://github.com/mcp2cli/source-code/issues/7), [#0](https://github.com/mcp2cli/source-code/issues/0))
- **site:** unify code-block chrome between docs and interactive terminal ([34ed1a1](https://github.com/mcp2cli/source-code/commit/34ed1a1))
- **site:** restore zsh-style colour palette + highlight interactive terminal commands ([0b23431](https://github.com/mcp2cli/source-code/commit/0b23431))
- **site:** wizard stays as scrollback after a pick, TUI-style ([33e9cbc](https://github.com/mcp2cli/source-code/commit/33e9cbc))
- **site:** always tail the terminal + favicon as header logo ([fb01314](https://github.com/mcp2cli/source-code/commit/fb01314))
- **site:** switch dark code theme from vesper to one-dark-pro ([#61](https://github.com/mcp2cli/source-code/issues/61), [#99](https://github.com/mcp2cli/source-code/issues/99), [#98](https://github.com/mcp2cli/source-code/issues/98), [#8](https://github.com/mcp2cli/source-code/issues/8), [#9](https://github.com/mcp2cli/source-code/issues/9), [#4](https://github.com/mcp2cli/source-code/issues/4))
- **site:** one-light theme + reliable terminal auto-tail ([#3360](https://github.com/mcp2cli/source-code/issues/3360), [#875](https://github.com/mcp2cli/source-code/issues/875), [#387138](https://github.com/mcp2cli/source-code/issues/387138), [#646568](https://github.com/mcp2cli/source-code/issues/646568), [#383](https://github.com/mcp2cli/source-code/issues/383))
- **site:** turn protocol coverage into a card grid with direction arrows ([d8a3305](https://github.com/mcp2cli/source-code/commit/d8a3305))
- **site:** script the interactive terminal to cycle through fresh picks ([b2450c8](https://github.com/mcp2cli/source-code/commit/b2450c8))
- **site:** TSOK attribution sentence in footer ([bf02eca](https://github.com/mcp2cli/source-code/commit/bf02eca))
- **site:** emit OTLP/HTTP spans for page_view, link_click, code_copied ([8071dac](https://github.com/mcp2cli/source-code/commit/8071dac))
- **telemetry:** wire real HTTP shipping to telemetry.mcp2cli.dev/ingest ([259f6bd](https://github.com/mcp2cli/source-code/commit/259f6bd))
- **telemetry:** OTLP/HTTP JSON + web→install→first-run attribution ([338d9b2](https://github.com/mcp2cli/source-code/commit/338d9b2))
- **telemetry:** decouple CLI telemetry from website / installer ([765810a](https://github.com/mcp2cli/source-code/commit/765810a))

### Fixes

- **ci:** pnpm version-conflict + formatting drift ([579246a](https://github.com/mcp2cli/source-code/commit/579246a))
- **site:** drop duplicate H1 on every docs page ([a6fe362](https://github.com/mcp2cli/source-code/commit/a6fe362))
- **site:** stop interactive terminal from scrolling the page when the wizard appears ([664039e](https://github.com/mcp2cli/source-code/commit/664039e))
- **site:** align interactive-terminal palette to Shiki's bash colours ([#6](https://github.com/mcp2cli/source-code/issues/6), [#005](https://github.com/mcp2cli/source-code/issues/005), [#99](https://github.com/mcp2cli/source-code/issues/99), [#032](https://github.com/mcp2cli/source-code/issues/032), [#7](https://github.com/mcp2cli/source-code/issues/7))
- **site:** finish aligning interactive-terminal palette with Shiki ([#7](https://github.com/mcp2cli/source-code/issues/7), [#8](https://github.com/mcp2cli/source-code/issues/8), [#6](https://github.com/mcp2cli/source-code/issues/6), [#616972](https://github.com/mcp2cli/source-code/issues/616972), [#032](https://github.com/mcp2cli/source-code/issues/032), [#99](https://github.com/mcp2cli/source-code/issues/99))
- **site:** dark-mode code blocks actually render dark colours ([2c5ef09](https://github.com/mcp2cli/source-code/commit/2c5ef09))
- **site:** give the MCP-spec icon visible hover feedback ([1dcb720](https://github.com/mcp2cli/source-code/commit/1dcb720))
- **site:** add vertical gap between adjacent code blocks ([f2a358d](https://github.com/mcp2cli/source-code/commit/f2a358d))
- **site:** anchor targets no longer hidden behind sticky header ([ff9cfbb](https://github.com/mcp2cli/source-code/commit/ff9cfbb))
- **site:** resolve relative .md and repo-source links in docs ([09c8b89](https://github.com/mcp2cli/source-code/commit/09c8b89))
- **site:** give the 'Copied!' tooltip readable foreground ([#383](https://github.com/mcp2cli/source-code/issues/383), [#141414](https://github.com/mcp2cli/source-code/issues/141414))

### Refactors

- **site:** smooth terminal auto-tail ([71d9d78](https://github.com/mcp2cli/source-code/commit/71d9d78))

### Docs

- improve source comments, remove internal docs, rewrite protocol coverage ([d8af816](https://github.com/mcp2cli/source-code/commit/d8af816))
- label bare code fences with inferred languages ([bfe3263](https://github.com/mcp2cli/source-code/commit/bfe3263))
- expose all three install flows on Getting Started + Install ([90d30ca](https://github.com/mcp2cli/source-code/commit/90d30ca))
- **install:** lead with the curl install.sh one-liner ([8387649](https://github.com/mcp2cli/source-code/commit/8387649))

### ❤️ Thank You

- Andrii Tsok