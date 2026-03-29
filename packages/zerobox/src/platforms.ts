/** Map of `${process.platform}-${process.arch}` to npm package name. */
export const PLATFORMS: Record<string, string> = {
  "darwin-arm64": "@zerobox/cli-darwin-arm64",
  "darwin-x64": "@zerobox/cli-darwin-x64",
  "linux-arm64": "@zerobox/cli-linux-arm64",
  "linux-x64": "@zerobox/cli-linux-x64",
};
