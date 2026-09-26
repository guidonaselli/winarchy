# Contributing to Winarchy

Bug reports, ideas and pull requests are welcome. Open an issue from one of the templates;
for anything bigger than a small fix, open the issue before writing code so we can agree on
the approach first.

## Branches

- `main` is where development happens. Pull requests go here.
- `release` is what users install and what `winarchy update --self` pulls. It only moves
  forward to tagged versions (see [docs/RELEASING.md](docs/RELEASING.md)).

## Before opening a pull request

- Run `.\tests\Invoke-WinarchyChecks.ps1` (needs PowerShell 7 and Pester 5+). CI runs the
  same thing.
- Read the invariants in [AGENTS.md](AGENTS.md). The short version: AutoHotkey owns every
  global hotkey, komorebi never runs elevated, and generated configs are edited through
  `templates/`, never by hand.
- Commit titles start with `feat:`, `fix:`, `chore:` or `refactor:`.
- Keep a pull request to one change. Link its issue with `Ref #123`.

## Labels

| Label | Meaning |
|---|---|
| `type: bug`, `type: feature`, `type: docs`, `type: question` | What kind of issue it is. Set by the template. |
| `area: ...` | Which part of Winarchy it touches. |
| `status: needs triage` | New, nobody has looked at it yet. |
| `status: needs info` | Waiting on the reporter. |
| `status: confirmed` | Reproduced or accepted; ready to be worked on. |
| `good first issue` | Small and self-contained, good for a first contribution. |
| `help wanted` | Accepted, but the maintainer won't get to it soon. |

Milestones are named after the version that ships the fix (`v1.7.0`).
