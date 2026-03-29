import type { SandboxOptions } from "./types.js";

/** Build CLI flags from SandboxOptions. */
export function buildFlags(options: SandboxOptions): string[] {
  const flags: string[] = [];

  if (options.allowAll) {
    flags.push("--allow-all");
    return flags;
  }

  if (options.noSandbox) {
    flags.push("--no-sandbox");
    return flags;
  }

  if (options.allowRead?.length) {
    flags.push(`--allow-read=${options.allowRead.join(",")}`);
  }
  if (options.denyRead?.length) {
    flags.push(`--deny-read=${options.denyRead.join(",")}`);
  }
  if (options.allowWrite?.length) {
    flags.push(`--allow-write=${options.allowWrite.join(",")}`);
  }
  if (options.denyWrite?.length) {
    flags.push(`--deny-write=${options.denyWrite.join(",")}`);
  }

  if (options.allowNet === true) {
    flags.push("--allow-net");
  } else if (Array.isArray(options.allowNet) && options.allowNet.length > 0) {
    flags.push(`--allow-net=${options.allowNet.join(",")}`);
  }
  if (options.denyNet?.length) {
    flags.push(`--deny-net=${options.denyNet.join(",")}`);
  }

  if (options.cwd) {
    flags.push("-C", options.cwd);
  }

  return flags;
}
