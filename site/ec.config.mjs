import { defineEcConfig } from 'astro-expressive-code';

// Monochrome themes so every fenced code block matches the
// interactive terminal on the landing: uniform foreground, muted
// comments, no accent colours for functions / strings / keywords /
// numbers. The only colour difference on the page is the scaffolding
// (chrome, copy button) — content reads as plain text.
//
// Themes are expressed as Shiki theme objects. Scope coverage
// matches the common TextMate scopes that would otherwise pull in
// accent colours; anything not listed inherits the `foreground`
// setting, i.e. the same shade as the commands in the hero terminal.

const commentScopes = [
  'comment',
  'punctuation.definition.comment',
  'string.comment',
];

const mcp2cliLight = {
  name: 'mcp2cli-light',
  type: 'light',
  settings: [
    { settings: { background: '#eef2f7', foreground: '#24292e' } },
    { scope: commentScopes, settings: { foreground: '#6a737d' } },
  ],
};

const mcp2cliDark = {
  name: 'mcp2cli-dark',
  type: 'dark',
  settings: [
    { settings: { background: '#0a0a0a', foreground: '#ffffff' } },
    { scope: commentScopes, settings: { foreground: '#7a7a7a' } },
  ],
};

export default defineEcConfig({
  themes: [mcp2cliLight, mcp2cliDark],
  themeCssSelector: (theme) =>
    theme.name === 'mcp2cli-dark' ? 'html.dark' : 'html:not(.dark)',
  styleOverrides: {
    borderRadius: '0.5rem',
    borderColor: 'var(--code-hairline)',
    codeBackground: 'var(--code-bg)',
    codeFontFamily:
      'ui-monospace, SFMono-Regular, Menlo, Monaco, "JetBrains Mono", monospace',
    codeFontSize: '13px',
    codeLineHeight: '1.55',
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
