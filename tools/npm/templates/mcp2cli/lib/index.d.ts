export interface RunOptions {
  /** Command name the CLI presents itself as. */
  name?: string;
  /** Path to the CLI's config file. */
  config?: string;
  /** Version `--version` reports — normally the publishing package's version. */
  version?: string;
  /** Arguments for the runtime. Defaults to `process.argv.slice(2)`. */
  args?: string[];
  /** Extra environment variables for the runtime. */
  env?: Record<string, string>;
}

/** Platform key (`<process.platform>-<process.arch>`) → binary package name. */
export const PLATFORM_PACKAGES: Record<string, string>;

/**
 * Absolute path of the mcp2cli binary for this platform.
 * Honors `MCP2CLI_BINARY`. Throws when no binary is available.
 */
export function binaryPath(): string;

/** Run mcp2cli with inherited stdio and exit the process with its status. */
export function run(options?: RunOptions): void;
