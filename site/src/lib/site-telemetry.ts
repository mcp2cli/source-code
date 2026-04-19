/**
 * Browser-side OpenTelemetry for mcp2cli.dev.
 *
 * Emits OTLP/HTTP JSON spans to the same OTEL Collector the CLI
 * uses (`https://otel.mcp2cli.dev/v1/traces`), but with its own
 * `service.name = "mcp2cli-site"` and a session-scoped anonymous
 * session id. No identifier leaks to the CLI or the installer —
 * each surface has its own independent telemetry stream.
 *
 * Opt-out signals respected before any span is ever emitted:
 *   - `navigator.doNotTrack === '1'`
 *   - `localStorage.mcp2cli_no_track === '1'`
 *   - running outside `mcp2cli.dev` (dev previews stay silent)
 */

import { trace } from '@opentelemetry/api';
import { resourceFromAttributes } from '@opentelemetry/resources';
import { BatchSpanProcessor } from '@opentelemetry/sdk-trace-base';
import { WebTracerProvider } from '@opentelemetry/sdk-trace-web';
import { OTLPTraceExporter } from '@opentelemetry/exporter-trace-otlp-http';

const OTLP_ENDPOINT = 'https://otel.mcp2cli.dev/v1/traces';
const SERVICE_NAME = 'mcp2cli-site';
const TRACER_NAME = 'mcp2cli.site';

let initialized = false;

function optedOut(): boolean {
  if (typeof navigator !== 'undefined' && navigator.doNotTrack === '1') return true;
  try {
    return localStorage.getItem('mcp2cli_no_track') === '1';
  } catch {
    return false;
  }
}

function getSessionId(): string {
  try {
    let sid = sessionStorage.getItem('mcp2cli_session');
    if (!sid) {
      sid =
        typeof crypto !== 'undefined' && 'randomUUID' in crypto
          ? crypto.randomUUID()
          : Math.random().toString(36).slice(2) + Date.now().toString(36);
      sessionStorage.setItem('mcp2cli_session', sid);
    }
    return sid;
  } catch {
    return '';
  }
}

function shouldEmit(): boolean {
  if (typeof window === 'undefined') return false;
  if (optedOut()) return false;
  // Stay silent on dev previews / local builds — only the real site
  // should produce events.
  if (window.location.hostname !== 'mcp2cli.dev') return false;
  return true;
}

export function initSiteTelemetry(): void {
  if (initialized) return;
  initialized = true;
  if (!shouldEmit()) return;

  const sessionId = getSessionId();

  const provider = new WebTracerProvider({
    resource: resourceFromAttributes({
      'service.name': SERVICE_NAME,
      'service.version': '0.1.0',
      'session.id': sessionId,
    }),
    spanProcessors: [
      new BatchSpanProcessor(
        new OTLPTraceExporter({ url: OTLP_ENDPOINT }),
        {
          maxExportBatchSize: 20,
          scheduledDelayMillis: 1500,
          exportTimeoutMillis: 4000,
        },
      ),
    ],
  });
  provider.register();

  const tracer = trace.getTracer(TRACER_NAME);

  // page_view — one span per page load.
  const pv = tracer.startSpan('page_view');
  pv.setAttribute('page.path', window.location.pathname);
  if (document.referrer) pv.setAttribute('page.referrer', document.referrer);
  if (document.title) pv.setAttribute('page.title', document.title);
  pv.end();

  // link_click — grouped: we only record path + link text, never
  // free-form input.
  document.addEventListener(
    'click',
    (e) => {
      const target = e.target as HTMLElement | null;
      const anchor = target?.closest('a') as HTMLAnchorElement | null;
      if (!anchor || !anchor.href) return;
      const span = tracer.startSpan('link_click');
      span.setAttribute('page.path', window.location.pathname);
      span.setAttribute('link.href', anchor.href);
      const text = anchor.textContent?.trim().slice(0, 80) ?? '';
      if (text) span.setAttribute('link.text', text);
      // Is this an outbound link? Helpful for distinguishing
      // "clicked GitHub" from "clicked docs".
      try {
        const u = new URL(anchor.href, window.location.origin);
        span.setAttribute(
          'link.external',
          u.hostname !== window.location.hostname,
        );
      } catch {
        /* ignore malformed href */
      }
      span.end();
    },
    { capture: true, passive: true },
  );

  // code_copied — fires whether copy came from the expressive-code
  // copy button or a plain Ctrl+C over a code block.
  document.addEventListener(
    'copy',
    () => {
      const target = (document.activeElement ?? document.body) as HTMLElement;
      const code = target.closest('pre, .expressive-code');
      if (!code) return;
      const span = tracer.startSpan('code_copied');
      span.setAttribute('page.path', window.location.pathname);
      // Best-effort language hint from expressive-code's wrapper.
      const lang = target.closest('pre')?.getAttribute('data-language');
      if (lang) span.setAttribute('code.language', lang);
      span.end();
    },
    { capture: true, passive: true },
  );

  // Flush pending spans when the tab goes to background or closes,
  // so we don't drop the tail of a session.
  const flush = () => {
    provider.forceFlush().catch(() => {});
  };
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'hidden') flush();
  });
  window.addEventListener('pagehide', flush);
}
