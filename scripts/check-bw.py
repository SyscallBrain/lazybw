#!/usr/bin/env python3
"""Run the opt-in official CLI test without inheriting a personal vault or tokens."""

import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile


def run_isolated(command, bw_path=None, expected_version=None):
    with tempfile.TemporaryDirectory(prefix="lazybw-bw-compat-") as directory:
        home = Path(directory)
        appdata = home / "appdata"
        appdata.mkdir(mode=0o700)
        runtime = home / "runtime"
        runtime.mkdir(mode=0o700)
        env = {
            "PATH": os.environ.get("PATH", os.defpath),
            "HOME": directory,
            "BITWARDENCLI_APPDATA_DIR": str(appdata),
            "XDG_CONFIG_HOME": str(home / "config"),
            "XDG_DATA_HOME": str(home / "data"),
            "XDG_STATE_HOME": str(home / "state"),
            "XDG_RUNTIME_DIR": str(runtime),
            "NO_COLOR": "1",
        }
        if bw_path is not None:
            env["TEST_BW_PATH"] = str(bw_path)
            env["TEST_BW_EXPECTED_VERSION"] = expected_version
        with subprocess.Popen(
            command, env=env, stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
            start_new_session=True,
        ) as process:
            try:
                output, _ = process.communicate(timeout=120)
            except BaseException:
                # Kill the whole isolated process group: a hung version probe
                # can outlive the Rust harness when only that parent is killed.
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                process.communicate()
                raise
        print(output, end="", flush=True)
        if process.returncode == 0 and expected_version is not None:
            # libtest exits successfully when a filter matches no tests. A
            # renamed or newly ignored check must never turn this gate green.
            if "test result: ok. 1 passed; 0 failed; 0 ignored;" not in output:
                print("expected exactly one executed compatibility test", file=sys.stderr)
                return 1
        return process.returncode


def main():
    if len(sys.argv) != 3:
        sys.exit("usage: scripts/check-bw.py /path/to/official/bw EXPECTED_VERSION")
    bw_path = Path(sys.argv[1]).absolute()
    if not bw_path.is_file() or not os.access(bw_path, os.X_OK):
        sys.exit(f"not an executable: {bw_path}")
    # Compilation uses the normal Rust installation; only the actual test
    # subprocess gets a disposable HOME. Never pass inherited BW_* tokens.
    build_env = {k: v for k, v in os.environ.items() if not k.startswith(("BW_", "LAZYBW_"))}
    build = subprocess.run(
        ["cargo", "test", "--locked", "-p", "lazybw", "--test", "real_bw", "--no-run",
         "--message-format=json"],
        cwd=Path(__file__).resolve().parent.parent,
        env=build_env, stdout=subprocess.PIPE, text=True, check=True,
    )
    executables = [
        message["executable"]
        for line in build.stdout.splitlines()
        if (message := json.loads(line)).get("reason") == "compiler-artifact"
        and message.get("target", {}).get("name") == "real_bw" and message.get("executable")
    ]
    if len(executables) != 1:
        sys.exit("cargo did not produce exactly one real_bw test executable")
    return run_isolated(
        [executables[0], "--ignored", "--exact", "official_cli_logged_out_compatibility", "--nocapture"],
        bw_path, sys.argv[2],
    )


if __name__ == "__main__":
    sys.exit(main())
