import { visit, SKIP } from 'unist-util-visit';
import type { Root } from 'hast';

/**
 * Rehype plugin that removes the first `<h1>` from the rendered
 * document. DocsLayout.astro renders its own `<h1>` derived from
 * frontmatter or the first H1 text; without this plugin every docs
 * page shows the title twice (once in the layout header, once as the
 * leading heading of the markdown body).
 */
export function rehypeStripFirstH1() {
  return (tree: Root) => {
    let stripped = false;
    visit(tree, 'element', (node, index, parent) => {
      if (stripped) return;
      if (
        node.tagName === 'h1' &&
        parent &&
        typeof index === 'number' &&
        (parent as Root).type === 'root'
      ) {
        (parent as Root).children.splice(index, 1);
        stripped = true;
        return [SKIP, index];
      }
    });
  };
}
