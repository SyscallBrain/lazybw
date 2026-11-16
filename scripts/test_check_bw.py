"""The compatibility runner must not expose a developer's Bitwarden state."""

import importlib.util
import contextlib
import io
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch


class IsolationTests(unittest.TestCase):
    def load_runner(self):
        path = Path(__file__).with_name("check-bw.py")
        spec = importlib.util.spec_from_file_location("check_bw", path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module

    def test_child_gets_private_empty_state_and_cleanup_survives_failure(self):
        module = self.load_runner()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            personal = root / "personal"
            personal.mkdir()
            sentinel = personal / "data.json"
            sentinel.write_text("personal vault must stay untouched")
            observed = root / "observed"
            child = root / "child.py"
            child.write_text("""#!/usr/bin/env python3
import os, pathlib, sys
assert not any(k.startswith(('BW_', 'LAZYBW_')) for k in os.environ)
assert not any(k in os.environ for k in ('NODE_OPTIONS', 'PYTHONPATH', 'LD_PRELOAD'))
home = pathlib.Path(os.environ['HOME'])
appdata = pathlib.Path(os.environ['BITWARDENCLI_APPDATA_DIR'])
assert appdata.is_relative_to(home)
for key in ('XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'XDG_STATE_HOME', 'XDG_RUNTIME_DIR'):
    assert pathlib.Path(os.environ[key]).is_relative_to(home)
assert list(appdata.iterdir()) == []
assert home.stat().st_mode & 0o777 == 0o700
pathlib.Path(sys.argv[1]).write_text(str(home))
(appdata / 'created-by-child').write_text('disposable')
sys.exit(23)
""")
            child.chmod(0o700)
            with patch.dict(os.environ, {
                "HOME": str(personal), "BITWARDENCLI_APPDATA_DIR": str(personal),
                "BW_SESSION": "personal-session", "BW_CLIENTSECRET": "personal-secret",
                "BW_CLIENTID": "personal-client", "BW_PASSWORD": "personal-password",
                "BW_UNKNOWN_FUTURE_TOKEN": "also-private", "LAZYBW_BW": "personal-bw",
                "XDG_CONFIG_HOME": str(personal), "XDG_DATA_HOME": str(personal),
                "XDG_STATE_HOME": str(personal), "XDG_RUNTIME_DIR": str(personal),
                "NODE_OPTIONS": "--require=personal-module", "PYTHONPATH": str(personal),
                "LD_PRELOAD": str(personal / "library.so"),
            }):
                result = module.run_isolated([str(child), str(observed)])
            self.assertEqual(result, 23)
            self.assertFalse(Path(observed.read_text()).exists())
            self.assertEqual(sentinel.read_text(), "personal vault must stay untouched")

    def test_missing_skipped_or_failed_compatibility_check_cannot_report_success(self):
        module = self.load_runner()
        for output, exit_code in [
            ("", 0),
            ("test result: ok. 0 passed; 0 failed; 0 ignored;", 0),
            ("test result: ok. 0 passed; 0 failed; 1 ignored;", 0),
            ("test result: FAILED. 0 passed; 1 failed; 0 ignored;", 101),
            ("test result: ok. 1 passed; 0 failed; 0 ignored;", 23),
        ]:
            with self.subTest(output=output, exit_code=exit_code), \
                    contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                result = module.run_isolated(
                    [sys.executable, "-c", "import sys; print(sys.argv[1]); sys.exit(int(sys.argv[2]))",
                     output, str(exit_code)],
                    Path(sys.executable), "2025.9.0",
                )
                self.assertNotEqual(result, 0)

    def test_one_executed_compatibility_check_can_report_success(self):
        module = self.load_runner()
        with contextlib.redirect_stdout(io.StringIO()):
            result = module.run_isolated(
                [sys.executable, "-c", "print('test result: ok. 1 passed; 0 failed; 0 ignored;')"],
                Path(sys.executable), "2025.9.0",
            )
        self.assertEqual(result, 0)

    def test_timeout_cleans_private_state_and_stops_descendants(self):
        module = self.load_runner()
        with tempfile.TemporaryDirectory() as directory:
            observed = Path(directory) / "observed"
            child = Path(directory) / "timeout.py"
            child.write_text("""import os, pathlib, subprocess, sys, time
descendant = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)'],
                              stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
pathlib.Path(sys.argv[1]).write_text(str(descendant.pid) + '\\n' + os.environ['HOME'])
time.sleep(30)
""")
            # Keep a real subprocess and cleanup; shorten only the deadline.
            real_communicate = subprocess.Popen.communicate

            def short_communicate(process, *args, **kwargs):
                if kwargs.get("timeout") is not None:
                    kwargs["timeout"] = 1
                return real_communicate(process, *args, **kwargs)

            try:
                with patch.object(subprocess.Popen, "communicate", short_communicate), \
                        self.assertRaises(subprocess.TimeoutExpired):
                    module.run_isolated([sys.executable, str(child), str(observed)])
                pid, home = observed.read_text().splitlines()
                self.assertFalse(Path(home).exists())
                stat = Path(f"/proc/{pid}/stat")
                deadline = time.monotonic() + 2
                while True:
                    try:
                        if stat.read_text().split()[2] == "Z":
                            break
                    except FileNotFoundError:
                        break
                    if time.monotonic() >= deadline:
                        self.fail("timed-out compatibility runner left a descendant running")
                    time.sleep(0.01)
            finally:
                if observed.exists():
                    pid = int(observed.read_text().splitlines()[0])
                    try:
                        os.kill(pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass


if __name__ == "__main__":
    unittest.main()
