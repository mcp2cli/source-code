/** @type {import('tailwindcss').Config} */
import typography from '@tailwindcss/typography';

export default {
  content: ['./src/**/*.{astro,html,js,jsx,md,mdx,ts,tsx}'],
  darkMode: 'class',
  theme: {
    extend: {
      fontFamily: {
        sans: [
          'ui-sans-serif',
          'system-ui',
          '-apple-system',
          'Segoe UI',
          'Inter',
          'sans-serif',
        ],
        mono: [
          'ui-monospace',
          'SFMono-Regular',
          'Menlo',
          'Monaco',
          'JetBrains Mono',
          'monospace',
        ],
      },
      colors: {
        border: 'hsl(var(--border))',
        ring: 'hsl(var(--ring))',
        background: 'hsl(var(--background))',
        foreground: 'hsl(var(--foreground))',
        muted: {
          DEFAULT: 'hsl(var(--muted))',
          foreground: 'hsl(var(--muted-foreground))',
        },
        subtle: 'hsl(var(--subtle))',
      },
      borderRadius: {
        lg: 'var(--radius)',
        md: 'calc(var(--radius) - 2px)',
        sm: 'calc(var(--radius) - 4px)',
      },
      typography: (theme) => ({
        DEFAULT: {
          css: {
            '--tw-prose-body': 'hsl(var(--foreground))',
            '--tw-prose-headings': 'hsl(var(--foreground))',
            '--tw-prose-lead': 'hsl(var(--foreground))',
            '--tw-prose-links': 'hsl(var(--foreground))',
            '--tw-prose-bold': 'hsl(var(--foreground))',
            '--tw-prose-counters': 'hsl(var(--muted-foreground))',
            '--tw-prose-bullets': 'hsl(var(--muted-foreground))',
            '--tw-prose-hr': 'hsl(var(--border))',
            '--tw-prose-quotes': 'hsl(var(--foreground))',
            '--tw-prose-quote-borders': 'hsl(var(--border))',
            '--tw-prose-captions': 'hsl(var(--muted-foreground))',
            '--tw-prose-kbd': 'hsl(var(--foreground))',
            '--tw-prose-code': 'hsl(var(--foreground))',
            '--tw-prose-pre-code': 'hsl(var(--foreground))',
            '--tw-prose-pre-bg': 'hsl(var(--subtle))',
            '--tw-prose-th-borders': 'hsl(var(--border))',
            '--tw-prose-td-borders': 'hsl(var(--border))',
            maxWidth: '72ch',
            a: {
              textDecoration: 'underline',
              textUnderlineOffset: '3px',
              textDecorationThickness: '1px',
              textDecorationColor: 'hsl(var(--muted-foreground))',
              fontWeight: '500',
              '&:hover': {
                textDecorationColor: 'hsl(var(--foreground))',
              },
            },
            'code::before': { content: '""' },
            'code::after': { content: '""' },
            code: {
              fontWeight: '500',
              backgroundColor: 'hsl(var(--subtle))',
              padding: '0.15rem 0.35rem',
              borderRadius: '0.25rem',
              border: '1px solid hsl(var(--border))',
            },
            'pre code': {
              backgroundColor: 'transparent',
              border: 'none',
              padding: 0,
            },
            pre: {
              border: '1px solid hsl(var(--border))',
              borderRadius: '0.5rem',
            },
            h1: { fontWeight: '600', letterSpacing: '-0.01em' },
            h2: { fontWeight: '600', letterSpacing: '-0.01em' },
            h3: { fontWeight: '600' },
            h4: { fontWeight: '600' },
          },
        },
      }),
    },
  },
  plugins: [typography],
};
