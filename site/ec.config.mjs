import { defineEcConfig } from 'astro-expressive-code';

// Colourful zsh-style themes so every code block and the interactive
// terminal share a vivid-on-contrast palette instead of flat text.
// - light: github-light — blue / purple / navy / red on off-white
// - dark:  vesper        — near-white text with peach functions,
//                          cyan strings, muted comment-grey
// Both write CSS vars for common tokens (--tok-cmd, --tok-flag,
// --tok-str, --tok-com) that the interactive terminal also reads, so
// the React demo can colour its own command lines to match.
export default defineEcConfig({
  themes: ['one-light', 'one-dark-pro'],
  // Use EC's attribute-based theme selection — it emits
  // `[data-theme="github-light"]` / `[data-theme="one-dark-pro"]`
  // which are clean, mutually-exclusive selectors. BaseLayout.astro
  // keeps `<html>`'s data-theme in sync with `.dark` class for
  // Tailwind.
  themeCssSelector: (theme) => `[data-theme="${theme.name}"]`,
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
      tooltipSuccessForeground: 'var(--code-fg)',
    },
  },
  defaultProps: {
    wrap: true,
  },
});
