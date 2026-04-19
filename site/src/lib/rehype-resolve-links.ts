import { visit } from 'unist-util-visit';
import path from 'node:path';
import type { Root, Element, Properties } from 'hast';
import type { VFile } from 'vfile';

/**
 * Rewrite relative links emitted by the source Markdown so they
 * work on the rendered docs site.
 *
 * Two classes of relative link show up in docs/:
 *
 * 1. Sibling / parent `.md` references — e.g. `features/daemon-mode.md`
 *    from protocol-coverage.md, or `../reference/cli-reference.md`
 *    from features/background-jobs.md. These should become URLs
 *    under `/docs/…` (lowercased, without the `.md` extension —
 *    matches Astro's content-collection slug normalisation).
 *
 * 2. Paths that escape `docs/` — e.g. `../src/mcp/protocol.rs` in
 *    protocol-coverage.md. These point at the repository's source
 *    code and should become absolute GitHub blob URLs.
 *
 * Everything else (anchors-only, absolute `/docs/...`, external
 * http(s) / mailto, etc.) is left untouched.
 */
const SOURCE_REPO_BASE = 'https://github.com/mcp2cli/source-code/blob/main';
const DOCS_URL_BASE = '/docs';

export function rehypeResolveLinks() {
  return (tree: Root, file: VFile) => {
    const absPath = (file.path ?? '').replace(/\\/g, '/');
    const marker = '/docs/';
    const markerIdx = absPath.lastIndexOf(marker);
    if (markerIdx < 0) return;
    const relPath = absPath.slice(markerIdx + marker.length);
    const currentDir = path.posix.dirname(relPath);

    visit(tree, 'element', (node: Element) => {
      if (node.tagName !== 'a') return;
      const props = node.properties as Properties | undefined;
      if (!props) return;
      const rawHref = props.href;
      if (typeof rawHref !== 'string' || rawHref.length === 0) return;

      // Skip anything that isn't a plain relative reference.
      if (/^(https?:|mailto:|tel:|ftp:|#|\/)/i.test(rawHref)) return;

      // Split the fragment off so we can resolve the path portion alone.
      const hashIdx = rawHref.indexOf('#');
      const linkPath = hashIdx >= 0 ? rawHref.slice(0, hashIdx) : rawHref;
      const frag = hashIdx >= 0 ? rawHref.slice(hashIdx) : '';
      if (!linkPath) return;

      const baseDir = currentDir === '.' ? '' : currentDir;
      const joined = path.posix.normalize(
        path.posix.join(baseDir || '.', linkPath),
      );

      if (joined.startsWith('..')) {
        // Escapes docs/. Treat as a repo-root-relative path and
        // rewrite to a GitHub blob URL.
        const repoPath = joined.replace(/^(?:\.\.\/)+/, '');
        props.href = `${SOURCE_REPO_BASE}/${repoPath}${frag}`;
        return;
      }

      // Inside docs/. Drop `.md` (Astro slugs omit it) and
      // lowercase to match Astro's content-collection IDs.
      const stripped = joined.endsWith('.md') ? joined.slice(0, -3) : joined;
      const slug = stripped.toLowerCase();
      props.href = `${DOCS_URL_BASE}/${slug}${frag}`;
    });
  };
}
