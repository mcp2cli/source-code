import { defineEcConfig } from 'astro-expressive-code';

export default defineEcConfig({
  themes: ['github-light', 'github-dark-dimmed'],
  themeCssSelector: (theme) =>
    theme.name === 'github-dark-dimmed' ? 'html.dark' : 'html:not(.dark)',
  styleOverrides: {
    borderRadius: '0.5rem',
    borderColor: 'hsl(var(--border))',
    codeFontFamily:
      'ui-monospace, SFMono-Regular, Menlo, Monaco, "JetBrains Mono", monospace',
    codeFontSize: '0.85rem',
    uiFontFamily:
      'ui-sans-serif, system-ui, -apple-system, "Segoe UI", Inter, sans-serif',
    frames: {
      shadowColor: 'transparent',
    },
  },
  defaultProps: {
    wrap: true,
  },
});
