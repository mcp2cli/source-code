import { visit } from 'unist-util-visit';
import type { Root } from 'mdast';

/**
 * Remark plugin that turns ```mermaid code fences into raw
 * `<pre class="mermaid">…</pre>` HTML nodes BEFORE the syntax
 * highlighter (expressive-code / Shiki) sees them, so mermaid
 * blocks pass through untouched and the client-side mermaid
 * script in DocsLayout can render them as SVG.
 */
export function remarkMermaid() {
  return (tree: Root) => {
    visit(tree, 'code', (node, index, parent) => {
      if (!parent || typeof index !== 'number') return;
      if (node.lang !== 'mermaid') return;

      const escaped = node.value
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;');

      parent.children[index] = {
        type: 'html',
        value: `<pre class="mermaid not-prose">${escaped}</pre>`,
      };
    });
  };
}
