import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

// Serves docs/files/SKILL.md at /SKILL.md as raw markdown so Claude Code
// and other agents can consume it directly (curl / WebFetch) the same way
// they load local skill files from disk.
export const prerender = true;

export async function GET() {
  const source = fileURLToPath(
    new URL('../../../docs/files/SKILL.md', import.meta.url),
  );
  const body = await readFile(source, 'utf-8');
  return new Response(body, {
    headers: { 'Content-Type': 'text/markdown; charset=utf-8' },
  });
}
