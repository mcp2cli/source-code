import { useCallback, useEffect, useRef, useState } from 'react';

// -------------------------------------------------------------
// Types / DSL
// -------------------------------------------------------------

type Line =
  | { kind: 'prompt'; text: string; comment?: string }
  | { kind: 'output'; text: string; tone?: 'muted' | 'ok' | 'warn' }
  | { kind: 'blank' };

type WizardOption = {
  id: string;
  label: string;
  hint?: string;
  run: Line[];
};

type Step =
  | { kind: 'commands'; lines: Line[] }
  | { kind: 'wizard'; question: string; options: WizardOption[] };

// -------------------------------------------------------------
// Script
// -------------------------------------------------------------

const INSTALL_SCRIPT: Step = {
  kind: 'commands',
  lines: [
    {
      kind: 'prompt',
      text: 'curl -fsSL https://mcp2cli.dev/install.sh | sh',
      comment: '# install',
    },
    { kind: 'output', text: 'mcp2cli installing from github.com/mcp2cli/source-code (branch main)', tone: 'muted' },
    { kind: 'output', text: '   Compiling mcp2cli v0.1.0', tone: 'muted' },
    { kind: 'output', text: '    Finished `release` profile [optimized]', tone: 'muted' },
    { kind: 'output', text: 'ok      installed: ~/.cargo/bin/mcp2cli', tone: 'ok' },
    { kind: 'blank' },
    {
      kind: 'prompt',
      text: 'mcp2cli config init --name work \\\n    --transport streamable_http \\\n    --endpoint http://127.0.0.1:3001/mcp',
    },
    { kind: 'output', text: 'Config "work" created at ~/.config/mcp2cli/configs/work.yaml', tone: 'muted' },
    { kind: 'blank' },
    { kind: 'prompt', text: 'mcp2cli link create --name work' },
    { kind: 'output', text: 'Linked: ~/.local/bin/work → mcp2cli', tone: 'ok' },
  ],
};

const WIZARD_STEP: Step = {
  kind: 'wizard',
  question: 'What would you like to try?',
  options: [
    {
      id: 'ls',
      label: 'work ls',
      hint: 'Discover the server capabilities',
      run: [
        { kind: 'prompt', text: 'work ls' },
        { kind: 'output', text: 'tools:     echo  search  email.send  email.reply', tone: 'muted' },
        { kind: 'output', text: 'resources: file:///{path}  demo://readme', tone: 'muted' },
        { kind: 'output', text: 'prompts:   summarise  review-diff', tone: 'muted' },
      ],
    },
    {
      id: 'invoke',
      label: 'work email send',
      hint: 'Call a tool with typed --flags from JSON Schema',
      run: [
        { kind: 'prompt', text: 'work email send --to user@example.com --body "Meeting at 3"' },
        { kind: 'output', text: '✓ queued (delivery_id: a4f2d3…)', tone: 'ok' },
      ],
    },
    {
      id: 'get',
      label: 'work get',
      hint: 'Read a resource by URI',
      run: [
        { kind: 'prompt', text: 'work get file:///project/README.md' },
        { kind: 'output', text: '# Project', tone: 'muted' },
        { kind: 'output', text: '', tone: 'muted' },
        { kind: 'output', text: 'Onboarding docs for new engineers.', tone: 'muted' },
      ],
    },
    {
      id: 'doctor',
      label: 'work doctor',
      hint: 'Health check + capability intersection',
      run: [
        { kind: 'prompt', text: 'work doctor' },
        { kind: 'output', text: 'transport:   streamable_http', tone: 'muted' },
        { kind: 'output', text: 'initialize:  ok (1250 ms, server=mcp-server-everything 2025-11-25)', tone: 'muted' },
        { kind: 'output', text: 'capabilities: tools resources prompts completion logging', tone: 'muted' },
        { kind: 'output', text: 'ok      ready', tone: 'ok' },
      ],
    },
  ],
};

const SCRIPT: Step[] = [INSTALL_SCRIPT, WIZARD_STEP];

// -------------------------------------------------------------
// Timing
// -------------------------------------------------------------

const CHAR_DELAY_MS = 22;
const CHAR_JITTER_MS = 18;
const LINE_DELAY_MS = 90;
const PROMPT_PAUSE_MS = 260;

const jitterDelay = () =>
  CHAR_DELAY_MS + Math.floor(Math.random() * CHAR_JITTER_MS);

// -------------------------------------------------------------
// Tailwind arbitrary-value classes that map onto the shared
// code-block CSS variables (defined in globals.css, swapped per
// theme). Keeping them in one place so the interactive terminal
// stays in lockstep with expressive-code fences.
// -------------------------------------------------------------

const BODY_BG = 'bg-[color:var(--code-bg)]';
const TITLEBAR_BG = 'bg-[color:var(--code-chrome)]';
const BORDER = 'border-[color:var(--code-hairline)]';
const TEXT_FG = 'text-[color:var(--code-fg)]';
const TEXT_MUTED = 'text-[color:var(--code-fg-muted)]';
const TEXT_OK = 'text-[color:var(--code-ok)]';

// -------------------------------------------------------------
// Shell-command tokenizer — enough to get a zsh-ish vibe on each
// prompt line without pulling a full grammar in. The first
// non-whitespace token in the *line* is the command; everything
// else follows a small set of pattern rules. Multi-line commands
// (backslash continuations) restart the "first token" heuristic
// on each logical line.
// -------------------------------------------------------------

type Tok = { text: string; cls: string | null };

const URL_RE = /^(?:https?:\/\/|file:\/\/|\.?\.?\/|~\/)[\w\-./%?&=#:{}@~]*$/i;
const FLAG_RE = /^--?[A-Za-z][\w-]*/;
const STRING_RE = /^["'].*["']$/;
const OP_RE = /^(?:\||\|\||&&|;|>>|>|<<|<)$/;

function tokenizeShell(text: string): Tok[] {
  const out: Tok[] = [];
  const parts = text.split(/(\s+)/);
  let expectCommand = true; // next non-whitespace token is the command
  for (const part of parts) {
    if (part === '') continue;
    if (/^\s+$/.test(part)) {
      if (/\n/.test(part)) expectCommand = true;
      out.push({ text: part, cls: null });
      continue;
    }
    // Shell operators — after one of these, the next token is a
    // new command (`cmd1 | cmd2`, `cmd1 && cmd2`, etc.).
    if (OP_RE.test(part)) {
      out.push({ text: part, cls: 'tok-op' });
      expectCommand = true;
      continue;
    }
    if (expectCommand) {
      expectCommand = false;
      // A continuation line may start with a flag — let the flag
      // matcher re-classify it.
      if (FLAG_RE.test(part)) {
        out.push({ text: part, cls: 'tok-flag' });
      } else {
        out.push({ text: part, cls: 'tok-cmd' });
      }
      continue;
    }
    if (FLAG_RE.test(part)) {
      out.push({ text: part, cls: 'tok-flag' });
    } else if (STRING_RE.test(part)) {
      out.push({ text: part, cls: 'tok-str' });
    } else if (URL_RE.test(part)) {
      out.push({ text: part, cls: 'tok-url' });
    } else {
      out.push({ text: part, cls: null });
    }
  }
  return out;
}

function tokenClass(cls: string | null): string {
  switch (cls) {
    case 'tok-cmd':
      return 'text-[color:var(--tok-cmd)]';
    case 'tok-flag':
      return 'text-[color:var(--tok-flag)]';
    case 'tok-str':
      return 'text-[color:var(--tok-str)]';
    case 'tok-url':
      return 'text-[color:var(--tok-url)]';
    case 'tok-op':
      return 'text-[color:var(--tok-op)]';
    default:
      return TEXT_FG;
  }
}

function HighlightedCommand({ text }: { text: string }) {
  const tokens = tokenizeShell(text);
  return (
    <>
      {tokens.map((t, i) => {
        const cls = t.cls ? tokenClass(t.cls) : TEXT_FG;
        return (
          <span key={i} className={cls}>
            {t.text}
          </span>
        );
      })}
    </>
  );
}

// -------------------------------------------------------------
// Internal run model — a Run is a sequence of Line records that
// the component animates one char at a time.
// -------------------------------------------------------------

type Run = {
  id: string;
  lines: Line[];
};

// -------------------------------------------------------------
// Rendering helpers
// -------------------------------------------------------------

function LineBody({
  line,
  revealed,
  isActive,
}: {
  line: Line;
  revealed: string;
  isActive: boolean;
}) {
  if (line.kind === 'blank') {
    return <div className="h-[1.5em]" aria-hidden="true" />;
  }

  if (line.kind === 'output') {
    const toneClass =
      line.tone === 'ok' ? TEXT_OK : line.tone === 'warn' ? 'text-amber-400' : TEXT_MUTED;
    return (
      <div className={`whitespace-pre-wrap ${toneClass}`}>
        {revealed}
        {isActive ? <Caret /> : null}
      </div>
    );
  }

  // Prompt line.
  return (
    <div className="whitespace-pre-wrap">
      <span className={`${TEXT_MUTED} select-none`}>$ </span>
      {isActive ? (
        <>
          <span className={TEXT_FG}>{revealed}</span>
          <Caret />
        </>
      ) : (
        <>
          <HighlightedCommand text={revealed} />
          {line.comment ? (
            <span className={`text-[color:var(--tok-com)]`}>
              {'  ' + line.comment}
            </span>
          ) : null}
        </>
      )}
    </div>
  );
}

function Caret() {
  return (
    <span
      aria-hidden="true"
      className="ml-[1px] inline-block h-[1em] w-[0.55em] -translate-y-[1px] bg-[color:var(--code-fg)] align-middle animate-[terminal-blink_1.1s_step-end_infinite]"
    />
  );
}

// -------------------------------------------------------------
// Completed run: no animation, full content.
// -------------------------------------------------------------

function StaticLines({ lines }: { lines: Line[] }) {
  return (
    <div>
      {lines.map((line, i) => {
        if (line.kind === 'blank')
          return <div key={i} className="h-[1.5em]" aria-hidden="true" />;
        if (line.kind === 'output') {
          const tone =
            line.tone === 'ok' ? TEXT_OK : line.tone === 'warn' ? 'text-amber-400' : TEXT_MUTED;
          return (
            <div key={i} className={`whitespace-pre-wrap ${tone}`}>
              {line.text}
            </div>
          );
        }
        return (
          <div key={i} className="whitespace-pre-wrap">
            <span className={`${TEXT_MUTED} select-none`}>$ </span>
            <HighlightedCommand text={line.text} />
            {line.comment ? (
              <span className={`text-[color:var(--tok-com)]`}>
                {'  ' + line.comment}
              </span>
            ) : null}
          </div>
        );
      })}
    </div>
  );
}

// -------------------------------------------------------------
// The animated run: one Line at a time, char by char.
// -------------------------------------------------------------

function AnimatedRun({
  run,
  onDone,
  skip,
}: {
  run: Run;
  onDone: () => void;
  skip: boolean;
}) {
  const [lineIdx, setLineIdx] = useState(0);
  const [charIdx, setCharIdx] = useState(0);

  useEffect(() => {
    // Reset when the run changes.
    setLineIdx(0);
    setCharIdx(0);
  }, [run.id]);

  useEffect(() => {
    if (skip) {
      if (lineIdx < run.lines.length) {
        setLineIdx(run.lines.length);
      } else {
        onDone();
      }
      return;
    }

    if (lineIdx >= run.lines.length) {
      onDone();
      return;
    }

    const line = run.lines[lineIdx];

    if (line.kind === 'blank') {
      const t = setTimeout(() => {
        setLineIdx((i) => i + 1);
        setCharIdx(0);
      }, LINE_DELAY_MS);
      return () => clearTimeout(t);
    }

    if (charIdx < line.text.length) {
      const t = setTimeout(
        () => setCharIdx((c) => c + 1),
        line.kind === 'prompt' ? jitterDelay() : 4,
      );
      return () => clearTimeout(t);
    }

    const pause = line.kind === 'prompt' ? PROMPT_PAUSE_MS : LINE_DELAY_MS;
    const t = setTimeout(() => {
      setLineIdx((i) => i + 1);
      setCharIdx(0);
    }, pause);
    return () => clearTimeout(t);
  }, [lineIdx, charIdx, run, skip, onDone]);

  return (
    <div>
      {run.lines.slice(0, lineIdx).map((line, i) => (
        <LineBody
          key={`${run.id}-l-${i}`}
          line={line}
          revealed={line.kind === 'blank' ? '' : line.text}
          isActive={false}
        />
      ))}
      {lineIdx < run.lines.length ? (
        <LineBody
          key={`${run.id}-active`}
          line={run.lines[lineIdx]}
          revealed={
            run.lines[lineIdx].kind === 'blank'
              ? ''
              : run.lines[lineIdx].text.slice(0, charIdx)
          }
          isActive={run.lines[lineIdx].kind !== 'blank'}
        />
      ) : null}
    </div>
  );
}

// -------------------------------------------------------------
// Wizard menu with keyboard navigation.
// -------------------------------------------------------------

function Wizard({
  question,
  options,
  onPick,
  autoFocus,
}: {
  question: string;
  options: WizardOption[];
  onPick: (option: WizardOption) => void;
  autoFocus: boolean;
}) {
  const [selected, setSelected] = useState(0);
  const containerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    // `preventScroll: true` so focusing the wizard for keyboard nav
    // doesn't yank the page when the menu appears a few seconds into
    // the autoplay. Keyboard users can still Tab into it.
    if (autoFocus) containerRef.current?.focus({ preventScroll: true });
  }, [autoFocus]);

  const handleKey = useCallback(
    (e: React.KeyboardEvent<HTMLDivElement>) => {
      if (e.key === 'ArrowDown' || (e.key === 'Tab' && !e.shiftKey)) {
        e.preventDefault();
        setSelected((s) => (s + 1) % options.length);
      } else if (e.key === 'ArrowUp' || (e.key === 'Tab' && e.shiftKey)) {
        e.preventDefault();
        setSelected((s) => (s - 1 + options.length) % options.length);
      } else if (e.key === 'Enter') {
        e.preventDefault();
        onPick(options[selected]);
      } else if (/^[1-9]$/.test(e.key)) {
        const idx = parseInt(e.key, 10) - 1;
        if (idx < options.length) {
          e.preventDefault();
          onPick(options[idx]);
        }
      }
    },
    [options, onPick, selected],
  );

  return (
    <div
      ref={containerRef}
      tabIndex={0}
      onKeyDown={handleKey}
      className={`mt-3 rounded border ${BORDER} ${BODY_BG} p-3 outline-none focus-visible:ring-1 focus-visible:ring-[color:var(--code-fg-muted)]`}
    >
      <div className={`mb-2 ${TEXT_FG}`}>{question}</div>
      <ul className="space-y-0.5">
        {options.map((opt, i) => {
          const active = i === selected;
          return (
            <li key={opt.id}>
              <button
                type="button"
                onMouseEnter={() => setSelected(i)}
                onClick={() => onPick(opt)}
                className={`flex w-full items-baseline gap-2 rounded px-2 py-1 text-left transition ${
                  active
                    ? `${TITLEBAR_BG} ${TEXT_FG}`
                    : `${TEXT_MUTED} hover:${TEXT_FG}`
                }`}
              >
                <span
                  aria-hidden="true"
                  className={`w-3 text-center ${active ? TEXT_OK : 'text-transparent'}`}
                >
                  ▸
                </span>
                <span className="font-medium">{opt.label}</span>
                {opt.hint ? <span className={TEXT_MUTED}>— {opt.hint}</span> : null}
              </button>
            </li>
          );
        })}
      </ul>
      <div className={`mt-3 text-[11px] ${TEXT_MUTED} opacity-80`}>
        ↑↓ select · Enter to run · 1–{options.length} shortcut
      </div>
    </div>
  );
}

// -------------------------------------------------------------
// Top-level component
// -------------------------------------------------------------

type Phase =
  | { kind: 'running'; run: Run; stepIdx: number }
  | { kind: 'wizard'; stepIdx: number }
  | { kind: 'done' };

export default function InteractiveTerminal() {
  const [history, setHistory] = useState<Run[]>([]);
  const [phase, setPhase] = useState<Phase>(() =>
    SCRIPT[0].kind === 'commands'
      ? { kind: 'running', run: { id: 'intro', lines: (SCRIPT[0] as { lines: Line[] }).lines }, stepIdx: 0 }
      : { kind: 'wizard', stepIdx: 0 },
  );
  const [runToken, setRunToken] = useState(0);
  const [skip, setSkip] = useState(false);
  const scrollRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
  });

  const advanceFromRun = useCallback(() => {
    const next = SCRIPT[(phase as { stepIdx: number }).stepIdx + 1];
    if (!next) {
      if (phase.kind === 'running') {
        setHistory((h) => [...h, phase.run]);
      }
      setPhase({ kind: 'done' });
      return;
    }
    if (phase.kind === 'running') {
      setHistory((h) => [...h, phase.run]);
    }
    if (next.kind === 'wizard') {
      setPhase({ kind: 'wizard', stepIdx: (phase as { stepIdx: number }).stepIdx + 1 });
    } else {
      setPhase({
        kind: 'running',
        run: { id: `step-${(phase as { stepIdx: number }).stepIdx + 1}`, lines: next.lines },
        stepIdx: (phase as { stepIdx: number }).stepIdx + 1,
      });
    }
  }, [phase]);

  const handlePick = useCallback(
    (opt: WizardOption) => {
      if (phase.kind !== 'wizard') return;
      setRunToken((n) => n + 1);
      setPhase({
        kind: 'running',
        run: { id: `${opt.id}-${Date.now()}`, lines: opt.run },
        stepIdx: phase.stepIdx,
      });
    },
    [phase],
  );

  const handleRunDone = useCallback(() => {
    if (phase.kind !== 'running') return;
    const wizardStep = SCRIPT[phase.stepIdx];
    if (wizardStep && wizardStep.kind === 'wizard') {
      setHistory((h) => [...h, phase.run]);
      setPhase({ kind: 'wizard', stepIdx: phase.stepIdx });
    } else {
      advanceFromRun();
    }
  }, [phase, advanceFromRun]);

  const restart = useCallback(() => {
    setHistory([]);
    setSkip(false);
    setRunToken((n) => n + 1);
    const first = SCRIPT[0];
    if (first.kind === 'commands') {
      setPhase({
        kind: 'running',
        run: { id: `intro-${Date.now()}`, lines: first.lines },
        stepIdx: 0,
      });
    } else {
      setPhase({ kind: 'wizard', stepIdx: 0 });
    }
  }, []);

  const handleBodyClick = useCallback(() => {
    if (phase.kind === 'running') setSkip(true);
  }, [phase]);

  return (
    <div className={`relative overflow-hidden rounded-lg border ${BORDER} ${BODY_BG}`}>
      {/* Floating restart button — mirrors the position of the copy
          button on the sibling code blocks, so the interactive
          terminal reads as the same kind of surface with a different
          affordance. */}
      <button
        type="button"
        onClick={restart}
        aria-label="Restart demo"
        title="Restart"
        className={`absolute right-[0.35rem] top-[0.35rem] z-10 flex h-7 w-7 items-center justify-center rounded ${TEXT_MUTED} transition hover:bg-[color:var(--code-chrome)] hover:${TEXT_FG} focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[color:var(--code-fg-muted)]`}
      >
        <svg
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.75"
          strokeLinecap="round"
          strokeLinejoin="round"
          className="h-3.5 w-3.5"
          aria-hidden="true"
        >
          <path d="M3 12a9 9 0 0 1 15.5-6.5L21 8"></path>
          <path d="M21 3v5h-5"></path>
          <path d="M21 12a9 9 0 0 1-15.5 6.5L3 16"></path>
          <path d="M3 21v-5h5"></path>
        </svg>
      </button>

      {/* body */}
      <div
        ref={scrollRef}
        onClick={handleBodyClick}
        className={`h-[420px] overflow-y-auto overflow-x-auto px-4 py-4 font-mono text-[13px] leading-[1.55] ${TEXT_FG}`}
      >
        {history.map((run) => (
          <div key={run.id} className="mb-3">
            <StaticLines lines={run.lines} />
          </div>
        ))}

        {phase.kind === 'running' ? (
          <AnimatedRun
            key={runToken}
            run={phase.run}
            onDone={handleRunDone}
            skip={skip}
          />
        ) : null}

        {phase.kind === 'wizard' ? (
          <Wizard
            question={(SCRIPT[phase.stepIdx] as { question: string }).question}
            options={(SCRIPT[phase.stepIdx] as { options: WizardOption[] }).options}
            onPick={handlePick}
            autoFocus={history.length > 0}
          />
        ) : null}
      </div>
    </div>
  );
}
