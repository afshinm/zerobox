import { describe, it, expect } from "vitest";
import { SandboxCommandError } from "./errors.js";

describe("SandboxCommandError", () => {
  it("uses stderr as message when available", () => {
    const err = new SandboxCommandError({ code: 1, stdout: "", stderr: "permission denied\n" });
    expect(err.message).toBe("permission denied");
    expect(err.code).toBe(1);
    expect(err.name).toBe("SandboxCommandError");
  });

  it("uses stdout as message when stderr is empty", () => {
    const err = new SandboxCommandError({ code: 1, stdout: "some output\n", stderr: "" });
    expect(err.message).toBe("some output");
  });

  it("uses generic message when both are empty", () => {
    const err = new SandboxCommandError({ code: 42, stdout: "", stderr: "" });
    expect(err.message).toBe("command exited with code 42");
  });

  it("is an instance of Error", () => {
    const err = new SandboxCommandError({ code: 1, stdout: "", stderr: "" });
    expect(err).toBeInstanceOf(Error);
    expect(err).toBeInstanceOf(SandboxCommandError);
  });

  it("exposes stdout and stderr", () => {
    const err = new SandboxCommandError({ code: 1, stdout: "out", stderr: "err" });
    expect(err.stdout).toBe("out");
    expect(err.stderr).toBe("err");
  });
});
