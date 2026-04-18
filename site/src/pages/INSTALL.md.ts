import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

// Serves docs/files/INSTALL.md at /INSTALL.md as raw markdown so
// agents, package managers, and CI scripts can fetch the installation
// guide directly (curl, WebFetch, etc.) without going through the
// HTML-rendered /docs/files/install page.
export const prerender = true;

export async function GET() {
  const source = fileURLToPath(
    new URL('../../../docs/files/INSTALL.md', import.meta.url),
  );
  const body = await readFile(source, 'utf-8');
  return new Response(body, {
    headers: { 'Content-Type': 'text/markdown; charset=utf-8' },
  });
}
