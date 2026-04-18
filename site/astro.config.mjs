import { defineConfig } from 'astro/config';
import mdx from '@astrojs/mdx';
import react from '@astrojs/react';
import sitemap from '@astrojs/sitemap';
import tailwind from '@astrojs/tailwind';
import expressiveCode from 'astro-expressive-code';
import { remarkMermaid } from './src/lib/remark-mermaid';

// https://astro.build/config
export default defineConfig({
  site: 'https://mcp2cli.dev',
  base: '/',
  trailingSlash: 'never',
  integrations: [
    react(),
    // Expressive Code runs the code-block pipeline: Shiki-powered syntax
    // highlighting, copy button, dark/light theme sync, frame titles.
    // Options live in ./ec.config.mjs so the <Code> Astro component (used
    // by non-markdown templates) can pick them up too. Must come BEFORE
    // `mdx()` in the integration array.
    expressiveCode(),
    mdx(),
    tailwind({ applyBaseStyles: false }),
    sitemap(),
  ],
  markdown: {
    remarkPlugins: [remarkMermaid],
  },
  vite: {
    server: {
      fs: {
        // Allow serving files from one level up (for ../docs content).
        allow: ['..'],
      },
    },
  },
});
