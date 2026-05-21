# Zerobox TypeScript SDK

<p>
  <a href="https://www.npmjs.com/package/zerobox" target="_blank">
    <img src="https://img.shields.io/npm/v/zerobox?style=for-the-badge&labelColor=000000&label=npm" alt="Zerobox npm version" />
  </a>
  <a href="https://github.com/afshinm/zerobox/blob/main/LICENSE" target="_blank">
    <img src="https://img.shields.io/github/license/afshinm/zerobox?style=for-the-badge&labelColor=000000" alt="Zerobox license" />
  </a>
</p>

TypeScript / Node SDK for [zerobox](https://github.com/afshinm/zerobox). Sandbox any command with file, network, and credential controls.

```bash
npm install zerobox
```

Installing the package drops the `zerobox` CLI into your `node_modules/.bin/` and exposes a TypeScript SDK.

> For CLI usage, secrets concepts, the full flag reference, performance numbers, and platform support see the [main README](https://github.com/afshinm/zerobox).

## Quick start

```ts
import { Sandbox } from "zerobox";

const sandbox = Sandbox.create({ allowWrite: ["/tmp"] });
const output = await sandbox.sh`echo hello`.text();
```

## Commands

The SDK exposes three ways to run a command. Each returns a `ShellCommand` you terminate with `.text()`, `.json()`, or `.output()`.

### Shell (tagged template)

```ts
const name = "world";
await sandbox.sh`echo hello ${name}`.text();
```

### Inline JavaScript

```ts
const data = await sandbox.js`
  console.log(JSON.stringify({ sum: 1 + 2 }));
`.json<{ sum: number }>();
```

### Explicit command + args

```ts
await sandbox.exec("node", ["-e", "console.log('hi')"]).text();
```

## Results

| Method | On success | On non-zero exit |
| --- | --- | --- |
| `.text()` | Returns stdout as a string | Throws `SandboxCommandError` |
| `.json<T>()` | Parses stdout as JSON (typed) | Throws `SandboxCommandError` |
| `.output()` | Returns `{ code, stdout, stderr }` | Returns the same shape, never throws |

```ts
const data = await sandbox.sh`cat data.json`.json();
const result = await sandbox.sh`exit 42`.output();
// { code: 42, stdout: "", stderr: "" }
```

## Error handling

Non-zero exit codes throw `SandboxCommandError`:

```ts
import { Sandbox, SandboxCommandError } from "zerobox";

const sandbox = Sandbox.create();
try {
  await sandbox.sh`exit 1`.text();
} catch (e) {
  if (e instanceof SandboxCommandError) {
    console.log(e.code);
    console.log(e.stderr);
  }
}
```

## Bind mounts

Give the sandboxed process a private view of a directory by remapping a host
path to a different path inside the sandbox. Useful for per-project tmpdirs:

```ts
const sandbox = Sandbox.create({
  bindMounts: [
    { host: "/tmp/projects/abc123", sandbox: "/tmp" },
    { host: "/var/cache/pkg", sandbox: "/var/cache/pkg", readOnly: true },
  ],
});

await sandbox.sh`node app.js`.text();
```

Mounts apply in the order declared, so list parents before children. On Linux
(and WSL2) the CLI performs a real bind mount; on macOS, Windows, and WSL1 it
emits a one-line warning to stderr and runs the command without remapping.

## Secrets

Pass API keys that the sandboxed process never sees. The proxy substitutes the real value only for approved hosts.

```ts
const sandbox = Sandbox.create({
  secrets: {
    OPENAI_API_KEY: {
      value: process.env.OPENAI_API_KEY!,
      hosts: ["api.openai.com"],
    },
    GITHUB_TOKEN: {
      value: process.env.GITHUB_TOKEN!,
      hosts: ["api.github.com"],
    },
  },
});

await sandbox.sh`node agent.js`.text();
```

See the [main README](https://github.com/afshinm/zerobox#secrets) for how placeholder substitution works.

## Snapshots

Record filesystem changes and roll them back automatically:

```ts
const sandbox = Sandbox.create({
  allowWrite: ["."],
  restore: true,
});

await sandbox.sh`npm install`.text();
```

Record without rolling back:

```ts
const sandbox = Sandbox.create({
  allowWrite: ["."],
  snapshot: true,
  snapshotExclude: ["node_modules"],
});

await sandbox.sh`npm install`.text();
```

## Cancellation

Pass an `AbortSignal` to any terminator:

```ts
const controller = new AbortController();
setTimeout(() => controller.abort(), 1000);
await sandbox.sh`sleep 60`.text({ signal: controller.signal });
```

## Environment variables

```ts
const sandbox = Sandbox.create({
  env: { NODE_ENV: "production" },
  allowEnv: ["PATH", "HOME"],
  denyEnv: ["AWS_SECRET_ACCESS_KEY"],
});
```

See the [main README](https://github.com/afshinm/zerobox#environment-variables) for what's inherited by default and the CLI equivalents.

## Options

`Sandbox.create(options)` accepts a `SandboxOptions` object. All fields are optional.

| Field | Type | Description |
| --- | --- | --- |
| `profile` | `string \| string[]` | Named profile(s). A list merges left-to-right. Default `"workspace"`. |
| `allowRead` / `denyRead` | `string[]` | Readable / blocked paths. |
| `allowWrite` / `denyWrite` | `string[]` | Writable / blocked paths. |
| `allowNet` | `boolean \| string[]` | `true` allows all. A list restricts to those domains. |
| `denyNet` | `string[]` | Blocked domains. |
| `allowAll` | `boolean` | Full filesystem + network access. |
| `noSandbox` | `boolean` | Disable the sandbox entirely. |
| `strictSandbox` | `boolean` | Fail instead of falling back to weaker isolation. |
| `cwd` | `string` | Working directory. |
| `env` | `Record<string, string>` | Explicit env vars. |
| `allowEnv` | `boolean \| string[]` | Inherit parent env vars. |
| `denyEnv` | `string[]` | Blocked env vars. |
| `snapshot` | `boolean` | Record filesystem changes. |
| `restore` | `boolean` | Record and roll back after exit. Implies `snapshot`. |
| `snapshotPaths` / `snapshotExclude` | `string[]` | Tracked paths / excluded patterns. |
| `secrets` | `Record<string, SecretConfig>` | Secrets with per-host scopes. |
| `bindMounts` | `BindMount[]` | Remap host directories onto sandbox paths. Linux/WSL2 real mount; macOS/Windows/WSL1 warn and no-op. |
| `debug` | `boolean` | Print sandbox config to stderr. |

## Caveats

Node.js `fetch` does not respect `HTTPS_PROXY` by default. When running Node inside a sandbox with secrets, pass `--use-env-proxy` to the sandboxed command.

## Other SDKs

- [Python SDK](https://github.com/afshinm/zerobox/tree/main/sdks/python) (PyPI: `zerobox`)
- [Rust SDK](https://github.com/afshinm/zerobox/tree/main/crates/zerobox) (crates.io: `zerobox`)

## License

Apache-2.0
