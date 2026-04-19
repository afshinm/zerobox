# Zerobox Rust SDK

<p>
  <a href="https://crates.io/crates/zerobox" target="_blank">
    <img src="https://img.shields.io/crates/v/zerobox?style=for-the-badge&labelColor=000000&label=crates.io" alt="Zerobox crates.io version" />
  </a>
  <a href="https://github.com/afshinm/zerobox/blob/main/LICENSE" target="_blank">
    <img src="https://img.shields.io/github/license/afshinm/zerobox?style=for-the-badge&labelColor=000000" alt="Zerobox license" />
  </a>
</p>

Rust SDK for [zerobox](https://github.com/afshinm/zerobox). Sandbox any command with file, network, and credential controls.

```toml
[dependencies]
zerobox = "0.2"
```

The crate ships both a library (`zerobox::Sandbox`) and the `zerobox` binary.

> For CLI usage, secrets concepts, the full flag reference, performance numbers, and platform support see the [main README](https://github.com/afshinm/zerobox).

## Quick start

```rust
use zerobox::Sandbox;

let output = Sandbox::command("echo")
    .arg("hello")
    .allow_write("/tmp")
    .run()
    .await?;

println!("{}", String::from_utf8_lossy(&output.stdout));
println!("exit: {}", output.status);
```

## Execution modes

### Collect output

```rust
let output = Sandbox::command("cargo")
    .arg("build")
    .allow_write("/project/target")
    .run()
    .await?;
```

### Stream output

```rust
let mut child = Sandbox::command("cargo")
    .arg("build")
    .allow_write("/project/target")
    .allow_net(&["crates.io"])
    .spawn()
    .await?;

let stdout = child.stdout().unwrap();
let status = child.wait().await?;
```

### Inherit stdio (TTY passthrough)

```rust
let status = Sandbox::command("vim")
    .allow_write("/project")
    .status()
    .await?;
```

## Secrets

Pass API keys that the sandboxed process never sees. The proxy substitutes the real value only for approved hosts.

```rust
let output = Sandbox::command("node")
    .arg("agent.js")
    .secret("OPENAI_API_KEY", "sk-proj-123")
    .secret_host("OPENAI_API_KEY", "api.openai.com")
    .secret("GITHUB_TOKEN", "ghp-456")
    .secret_host("GITHUB_TOKEN", "api.github.com")
    .run()
    .await?;
```

See the [main README](https://github.com/afshinm/zerobox#secrets) for how placeholder substitution works.

## Environment variables

```rust
let output = Sandbox::command("node")
    .arg("app.js")
    .env("NODE_ENV", "production")
    .allow_env(&["PATH", "HOME"])
    .deny_env(&["AWS_SECRET_ACCESS_KEY"])
    .run()
    .await?;
```

## Profiles

```rust
// Default profile loads automatically.
let output = Sandbox::command("npm test").run().await?;

// Use a different profile.
let output = Sandbox::command("npm test")
    .profile("workspace")
    .run()
    .await?;

// Combine multiple profiles (merged left-to-right).
let output = Sandbox::command("claude")
    .profiles(&["claude", "git-config"])
    .run()
    .await?;

// Opt out of profiles.
let output = Sandbox::command("npm test")
    .no_profile()
    .allow_read("/src")
    .run()
    .await?;
```

## Full access / no sandbox

```rust
let output = Sandbox::command("install.sh")
    .full_access()
    .run()
    .await?;

let output = Sandbox::command("ls")
    .no_sandbox()
    .run()
    .await?;
```

## Builder reference

| Method | Description |
| --- | --- |
| `command(cmd)` | Start a new builder for `cmd`. |
| `arg(x)` / `args(xs)` | Append arguments. |
| `cwd(path)` | Working directory. |
| `allow_read(path)` / `deny_read(path)` | Readable / blocked paths. |
| `allow_write(path)` / `deny_write(path)` | Writable / blocked paths. |
| `allow_net(domains)` / `deny_net(domains)` | Allowed / blocked domains. Pass `&[]` for all. |
| `env(k, v)` | Set an env var. |
| `allow_env(keys)` / `deny_env(keys)` | Inherit / block parent env vars. |
| `secret(k, v)` / `secret_host(k, hosts)` | Secret and its allowed hosts. |
| `profile(name)` / `profiles(names)` / `no_profile()` | Select or skip profiles. |
| `full_access()` / `no_sandbox()` / `strict_sandbox()` | Coarse policy switches. |
| `snapshot()` / `restore()` | Record / roll back filesystem changes. |
| `run()` / `spawn()` / `status()` | Terminators (collect / stream / inherit stdio). |

## Other SDKs

- [TypeScript SDK](https://github.com/afshinm/zerobox/tree/main/packages/zerobox) (npm: `zerobox`)
- [Python SDK](https://github.com/afshinm/zerobox/tree/main/sdks/python) (PyPI: `zerobox`)

## License

Apache-2.0
