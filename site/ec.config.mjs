import { defineEcConfig } from 'astro-expressive-code';

// Single dark theme for every code block, regardless of the site's
// light/dark mode — gives every code fence a terminal look (black
// background, grey chrome) and keeps long commands readable against
// the neutral page palette.
export default defineEcConfig({
  themes: ['github-dark-dimmed'],
  styleOverrides: {
    borderRadius: '0.5rem',
    borderColor: '#1f1f1f',
    codeBackground: '#0a0a0a',
    codeFontFamily:
      'ui-monospace, SFMono-Regular, Menlo, Monaco, "JetBrains Mono", monospace',
    codeFontSize: '0.85rem',
    uiFontFamily:
      'ui-sans-serif, system-ui, -apple-system, "Segoe UI", Inter, sans-serif',
    frames: {
      shadowColor: 'transparent',
      editorBackground: '#0a0a0a',
      terminalBackground: '#0a0a0a',
      editorTabBarBackground: '#141414',
      terminalTitlebarBackground: '#141414',
      terminalTitlebarBorderBottomColor: '#1f1f1f',
      editorTabBarBorderBottomColor: '#1f1f1f',
      editorActiveTabIndicatorTopColor: 'transparent',
      editorActiveTabBackground: '#0a0a0a',
      tooltipSuccessBackground: '#141414',
    },
  },
  defaultProps: {
    wrap: true,
  },
});
