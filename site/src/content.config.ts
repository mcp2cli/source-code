import { defineCollection, z } from 'astro:content';
import { glob } from 'astro/loaders';

// Read all markdown docs from the repo-root docs/ directory. AGENTS.md is
// excluded because it's a contributor-internal instruction file; SKILL.md
// is excluded here and served as a raw agent skill file at /SKILL.md.
const docs = defineCollection({
  loader: glob({
    pattern: ['**/*.md', '!files/AGENTS.md', '!files/SKILL.md'],
    base: '../docs',
  }),
  schema: z
    .object({
      title: z.string().optional(),
      description: z.string().optional(),
    })
    .passthrough(),
});

export const collections = { docs };
