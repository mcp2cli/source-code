import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

// Serve docs/files/llms.txt at the canonical /llms.txt path — per the
// https://llmstxt.org convention for LLM-discoverable project summaries.
export const prerender = true;

export async function GET() {
  const source = fileURLToPath(
    new URL('../../../docs/files/llms.txt', import.meta.url),
  );
  const body = await readFile(source, 'utf-8');
  return new Response(body, {
    headers: { 'Content-Type': 'text/plain; charset=utf-8' },
  });
}
