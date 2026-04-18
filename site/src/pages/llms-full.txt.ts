import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

// Full-context sibling to /llms.txt. Serves docs/files/llms-full.txt as
// plain text so LLMs can fetch the complete project digest in one round-trip.
export const prerender = true;

export async function GET() {
  const source = fileURLToPath(
    new URL('../../../docs/files/llms-full.txt', import.meta.url),
  );
  const body = await readFile(source, 'utf-8');
  return new Response(body, {
    headers: { 'Content-Type': 'text/plain; charset=utf-8' },
  });
}
