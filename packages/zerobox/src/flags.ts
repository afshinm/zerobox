import type { SandboxOptions } from "./types.js";

/** Build CLI flags from SandboxOptions. */
export function buildFlags(options: SandboxOptions): string[] {
  const flags: string[] = [];

  // Collect secret hosts for network permission merging.
  const secretHosts: string[] = [];

  if (options.secrets) {
    for (const [key, config] of Object.entries(options.secrets)) {
      flags.push("--secret", `${key}=${config.value}`);
      if (config.hosts.length > 0) {
        flags.push("--secret-host", `${key}=${config.hosts.join(",")}`);
        secretHosts.push(...config.hosts);
      }
    }
  }

  if (options.strictSandbox) {
    flags.push("--strict-sandbox");
  }
  if (options.debug) {
    flags.push("--debug");
  }

  if (options.allowAll) {
    flags.push("--allow-all");
  } else if (options.noSandbox) {
    flags.push("--no-sandbox");
  } else {
    flags.push("--profile", options.profile ?? "workspace");
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

    // Merge secret hosts into allowNet (secrets auto-enable network for their hosts).
    // The CLI also does this, but we emit it here so --allow-net reflects the full picture.
    let effectiveAllowNet = options.allowNet;
    if (secretHosts.length > 0) {
      if (effectiveAllowNet === true) {
        // Already allowing all network.
      } else if (Array.isArray(effectiveAllowNet)) {
        effectiveAllowNet = [...effectiveAllowNet, ...secretHosts];
      } else {
        // Network was not explicitly enabled; secrets handle it via --secret-host.
        // Don't emit --allow-net here — the CLI enables network implicitly for secret hosts.
      }
    }

    if (effectiveAllowNet === true) {
      flags.push("--allow-net");
    } else if (Array.isArray(effectiveAllowNet) && effectiveAllowNet.length > 0) {
      flags.push(`--allow-net=${effectiveAllowNet.join(",")}`);
    }
    if (options.denyNet?.length) {
      flags.push(`--deny-net=${options.denyNet.join(",")}`);
    }
  }

  // Env flags — emitted for all modes (including allowAll).
  if (options.allowEnv === true) {
    flags.push("--allow-env");
  } else if (Array.isArray(options.allowEnv) && options.allowEnv.length > 0) {
    flags.push(`--allow-env=${options.allowEnv.join(",")}`);
  }
  if (options.denyEnv?.length) {
    flags.push(`--deny-env=${options.denyEnv.join(",")}`);
  }
  if (options.env) {
    for (const [key, value] of Object.entries(options.env)) {
      flags.push("--env", `${key}=${value}`);
    }
  }

  if (options.cwd) {
    flags.push("-C", options.cwd);
  }

  return flags;
}
