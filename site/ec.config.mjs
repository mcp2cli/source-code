import { defineEcConfig } from 'astro-expressive-code';

// Dual-theme code blocks: github-light in light mode, vesper in dark
// mode. `vesper` is Rauno's minimalist theme — near-pure-white
// foreground with highly restrained accents (soft cyan, muted orange,
// grey keywords). Text reads almost entirely white against the pinned
// #0a0a0a background, matching the site's minimal aesthetic.
// Backgrounds and chrome flow through CSS variables defined in
// globals.css (--code-bg / --code-chrome / --code-hairline) so both
// modes stay subtle against the page rather than shouting.
export default defineEcConfig({
  themes: ['github-light', 'vesper'],
  themeCssSelector: (theme) =>
    theme.name === 'vesper' ? 'html.dark' : 'html:not(.dark)',
  styleOverrides: {
    borderRadius: '0.5rem',
    borderColor: 'var(--code-hairline)',
    codeBackground: 'var(--code-bg)',
    codeFontFamily:
      'ui-monospace, SFMono-Regular, Menlo, Monaco, "JetBrains Mono", monospace',
    codeFontSize: '0.85rem',
    uiFontFamily:
      'ui-sans-serif, system-ui, -apple-system, "Segoe UI", Inter, sans-serif',
    frames: {
      shadowColor: 'transparent',
      editorBackground: 'var(--code-bg)',
      terminalBackground: 'var(--code-bg)',
      editorTabBarBackground: 'var(--code-chrome)',
      terminalTitlebarBackground: 'var(--code-chrome)',
      terminalTitlebarBorderBottomColor: 'var(--code-hairline)',
      editorTabBarBorderBottomColor: 'var(--code-hairline)',
      editorActiveTabIndicatorTopColor: 'transparent',
      editorActiveTabBackground: 'var(--code-bg)',
      tooltipSuccessBackground: 'var(--code-chrome)',
    },
  },
  defaultProps: {
    wrap: true,
  },
});
