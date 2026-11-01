# Security policy

lazybw handles the contents of your password vault, so security reports are
taken seriously.

## Reporting a vulnerability

Please report vulnerabilities privately through
[GitHub's security advisories](https://github.com/SyscallBrain/lazybw/security/advisories/new),
not in a public issue. Include the version (`lazybw --version`), the output
of `lazybw doctor` with your email removed, and the steps to reproduce.

You can expect a first answer within a week. Once a fix is released, the
advisory is published with credit to you, unless you prefer otherwise.

## Scope

lazybw is a front end for the official Bitwarden CLI. Problems in `bw`
itself or in Bitwarden's servers belong to
[Bitwarden's own programme on HackerOne](https://hackerone.com/bitwarden).

How lazybw protects secrets is described in the
[security chapter](https://syscallbrain.github.io/lazybw/security.html) of the
documentation.

## Supported versions

Only the latest release receives security fixes.
