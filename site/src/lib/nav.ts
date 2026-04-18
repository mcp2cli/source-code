import { getCollection, type CollectionEntry } from 'astro:content';

export type NavItem = {
  label: string;
  slug: string; // relative to /docs, e.g. 'features/daemon-mode'
};

export type NavSection = {
  title: string;
  items: NavItem[];
};

// Map a raw entry id (e.g. `features/daemon-mode`, `index`, `files/INSTALL`)
// to a human label and a final URL slug (which appears after `/docs/`).
function friendlyLabel(id: string): string {
  const last = id.split('/').pop() ?? id;
  if (last === 'index') return 'Overview';
  return last
    .replace(/^INSTALL$/i, 'Install')
    .replace(/\.md$/, '')
    .split('-')
    .map((segment) => {
      // Keep ALL-CAPS tokens like CLI, CI/CD, MCP, e2e intact.
      if (/^[A-Z0-9]{2,}$/.test(segment)) return segment;
      return segment.charAt(0).toUpperCase() + segment.slice(1);
    })
    .join(' ');
}

function toSlug(id: string): string {
  // 'index' at the collection root → '' so the URL is just /docs.
  return id === 'index' ? '' : id;
}

const START_ORDER = [
  'index',
  'files/INSTALL',
  'getting-started',
  'usage-guide',
  'use-cases',
];

const REFERENCE_ORDER = [
  'reference/cli-reference',
  'reference/config-reference',
];

export async function buildNav(): Promise<NavSection[]> {
  const entries = (await getCollection('docs')) as CollectionEntry<'docs'>[];
  const ids = new Set(entries.map((e) => e.id));

  const pick = (idList: string[]): NavItem[] =>
    idList
      .filter((id) => ids.has(id))
      .map((id) => ({ label: friendlyLabel(id), slug: toSlug(id) }));

  const collect = (prefix: string): NavItem[] =>
    entries
      .filter((e) => e.id.startsWith(`${prefix}/`))
      .map((e) => ({ label: friendlyLabel(e.id), slug: toSlug(e.id) }))
      .sort((a, b) => a.label.localeCompare(b.label));

  const sections: NavSection[] = [
    { title: 'Start', items: pick(START_ORDER) },
    { title: 'Protocol', items: pick(['protocol-coverage']) },
    { title: 'Features', items: collect('features') },
    { title: 'Reference', items: pick(REFERENCE_ORDER) },
    { title: 'Articles', items: collect('articles') },
    {
      title: 'More',
      items: pick(['telemetry-collection']),
    },
  ].filter((section) => section.items.length > 0);

  return sections;
}

export function isActive(currentSlug: string, itemSlug: string): boolean {
  if (itemSlug === '' && (currentSlug === '' || currentSlug === 'index'))
    return true;
  return currentSlug === itemSlug;
}
