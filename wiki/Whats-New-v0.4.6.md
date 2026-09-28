# What's New in v0.4.6

> 🇧🇷 [Versão em português](Novidades-v0.4.6) · 📋 [Full CHANGELOG](https://github.com/michelbr84/GarraRUST/blob/main/CHANGELOG.md) · 📦 [Download](https://github.com/michelbr84/GarraRUST/releases/tag/v0.4.6)

Two large fronts and a stabilization band.

The first is **Access Policy v2** ([ADR 0025](https://github.com/michelbr84/GarraRUST/blob/main/docs/adr/0025-classes-de-capacidade-e-politica-de-acesso-v2.md)):
who may talk to Garra over personal WhatsApp, and **how far each of them goes**,
becomes a single engine shared by the CLI, the admin API and a new Web Console
page — not three copies of the same rule, each wrong in its own way.

The second is **runtime honesty**: one capability registry now answers "what can
you actually do?" for `garra_status`, for `/api/diagnostics`, for the console and
for the model itself — with a state, a reason and a remediation. The model stops
saying "I don't have MCP" when MCP is merely hidden, and stops saying "that
doesn't exist" when the truth is "it exists and it isn't allowed".

Around that: the MCP `filesystem` server confined to the same jail as the native
file tools, a circuit breaker for tools that keep failing, the gateway becoming
an MCP server over HTTP, and CI that no longer depends on a registry.

---

## Access Policy v2 — who gets in, and how far (#1388, ADR 0025)

`channels.whatsapp_linked.access` is the new section. It answers four questions
that used to live scattered across `allow`, `owners` and `reply_in_groups`:

| Question | Where | Values |
|---|---|---|
| Who is admitted? | `access.admission` | `restricted` (list only) · `open` (anyone) |
| What does a stranger get? | `access.unknown_default` | starting level for unlisted senders |
| Do groups take part? | `access.groups` | on/off, plus a **per-group policy** |
| How far does each person go? | per-principal floor + ceiling | `chat` · `read` · `full`, with file writes separate |

The level is a **ceiling that composes with the session mode: it only removes,
never grants.** A number at `chat` gains no tool just because the session is in
`code` mode — policy and mode meet at the most restrictive point. Existing
`allow`/`owners` in `config.yml` keep working: v2 reads the legacy shape, and a
declared `access.groups.enabled` beats the old `reply_in_groups` (#1501). Turning
groups on and off takes effect on the **next message**, with no restart (#1412).

### From the CLI

```bash
garraia whatsapp access                      # the EFFECTIVE policy, via the turn's own engine
garraia whatsapp users --json                # who is authorized, by role
garraia whatsapp level +15551234567 read     # a number's ceiling: chat | read | full
garraia whatsapp level +15551234567 read --dry-run   # the impact, without writing
garraia whatsapp write +15551234567 on       # file writes, and only that
garraia whatsapp block +15551234567          # beats 'open', 'allow' and pairing
garraia whatsapp unblock +15551234567
garraia whatsapp owner +15551234567          # promote to owner (isolated-pod only)
garraia whatsapp unowner +15551234567        # demote WITHOUT revoking access
garraia whatsapp remove +15551234567         # revoke
```

Identities show up **only as their last four digits** on every screen and in
every `--json` — the full number stays in `config.yml` (mode `0600`). `--reveal`
prints the config values, locally. Every permission command leaves an audit
trail, including the legacy `allow`/`remove`/`owner`/`unowner` (#1414).

### From the Web Console

The **WhatsApp Access** page (#1402–#1405) is the same policy through the same
API: a per-principal matrix, **a preview of the effect before you confirm**, and
a phone number validated by the same rule the CLI uses (#1403) — not a second
validator that accepts what the CLI rejects. The matrix shows web and memory
alongside the other capabilities (#1411). The **Test WhatsApp** button runs the
**same engine** as `garraia doctor whatsapp` (#1420), and recent rejections are
visible (#1422).

### End-to-end diagnosis

```bash
garraia doctor whatsapp            # or --json
```

Walks the whole path (#1419): link, session key, gateway, access policy,
execution profile, workspace, MCP visibility and provider. Exit 0 when it
passes, 69 when something is unavailable — with the line that says what.

### The wizard now ends by showing the gate

`garraia whatsapp link` used to end with "ready to receive messages" without
saying who could actually talk. It now prints the same summary as
`garraia whatsapp access` — channel, execution profile, admission, stranger
default, groups, and each principal with floor, level and what it can really do
(#1429). And `garra init` now offers WhatsApp alongside Telegram (#1430).

---

## Runtime honesty: the capability registry (#1381, #1387, #1416)

A single function (`capacidades_registro::registro`) gathers the runtime
inventory, the turn's gate, each tool's availability, the state of every MCP
server, and whether `bash` is exposed. Every capability comes out with a
**state**, a machine- and human-readable reason, and a remediation:

| State | Meaning |
|---|---|
| `visible` | exists and is allowed |
| `denied` | **exists and is not allowed** — never "doesn't exist" |
| `unavailable` | exists, unusable right now (the remediation says why) |
| `unhealthy` | server registered, currently down |
| `not_configured` | never configured — different from broken |

The same registry feeds `garra_status` (which now returns `capabilities`),
`/api/diagnostics` (`tools.capabilities`) and `GET /admin/api/capabilities`. The
prompt note tells the model to **answer from it** and forbids inferring absence
from invisibility. The registry also reports missing context — whether the file
tools have a root in this session, and which repository is active (#1416, #1418)
— and `garra_status` explains that `withheld` means "held back by policy", not
"capability absent" (#1382).

A tool that is registered but **not operational drops off the callable list**
instead of being offered to the model and failing on call (#1425).

### Capability classes and a per-principal ceiling (#1385, #1392)

Every tool declares what it does in closed classes — `filesystem.read`,
`filesystem.write`, `process.execute`, `network.read`, `message.send`,
`device.*`, `memory.*`, `runtime.inspect`, `scheduling`, `mcp.read`,
`mcp.write`. Native tools via a closed table; MCP tools via the known filesystem
operation or the server's annotations. **No declared class means fail-closed in
restricted mode.** The principal's ceiling enters the `ExecContext` and composes
with the mode.

---

## Security

- **MCP `filesystem` confined to the session jail** (#1482, #1383). There were
  two gates with different rules inside the same turn: native file tools honored
  the per-session jail, MCP filesystem calls did not. Now one jail per session
  governs both.
- **An MCP tool no longer becomes a slash command** (#1386). Automatic
  registration exposed every MCP tool as `/command`, **outside the modes'
  `ToolGate`** — a bypass of the tool policy.
- **Session-by-id split into client and operator** (#1462). Channel session ids
  are guessable by construction (`whatsapp-linked-<number>`,
  `telegram-<chat>`). Writes were already closed; now reads are too: `GET
  /api/sessions`, `GET /api/sessions/{id}/history`, `DELETE /api/sessions/{id}`
  and the token-less `resume` on `/ws` only reach the operator's local surfaces
  (`api`, `vscode`, `web`, `parrot`). A channel or mobile session answers the
  same `404 session not found` as a nonexistent id, byte for byte, and is not
  hydrated, disconnected or listed. Operator reads moved to `GET
  /admin/api/sessions/{id}/history` (`/admin` cookie + `manage_sessions`;
  `viewer` gets 403).
- **`/api/diagnostics` no longer publishes absolute host paths** (#1465).
- **Phone numbers with common separators are now masked** (#1514) — masking
  covered the raw number and missed `+1 555 123-4567`.
- **Default workspace and per-session directories are created `0700`** (#1463).

## Workspace and project, per session

- **A session with no project gets a safe, per-session workspace** (#1378,
  #1449). With `agent.file_roots` empty — the default — the jail had *no root*,
  and no root denies everything: the file tools appeared registered and every
  call returned `NoRoots`. The effective root is now
  `<data_dir>/workspace/<session>` (name = SHA-256 of the `session_id`, never the
  id in the clear). Never `/`, never `$HOME`, never `pod_root`. The "per
  session" part comes from the independent review (#1449): the first version
  had a shared root, which would let one WhatsApp contact read what another
  wrote. A declared `agent.file_roots` still wins outright.
- **`/project` selects a project per session** (#1379, #1424). The path is
  re-confined by the operator's project roots (`GARRAIA_PROJECT_ROOTS`), becomes
  the `working_dir` for file tools and `repo_search`, and the binding persists in
  `sessions.db` — a `garraia restart` restores the project, and a project that
  fell outside the roots **does not come back** (fail-closed). The reply shows a
  name and a short id, never the path. On WhatsApp, a message starting with `/`
  is decided **per principal before it reaches the model**: `/help` for anyone
  admitted, `/project` for owner and user, `/mode` and `/goal` for the owner
  only. Selecting a project **does not change power** — the turn's gate is still
  mode + ceiling.
- **Roots resolved once, at boot** (#1459).

## Reliability

- **A per-session, per-tool circuit breaker** (#1417) at the single dispatch
  point: a deterministic failure pauses the tool until the end of the turn, a
  timeout opens a cooldown with exponential backoff 15s→120s, and a generic
  error only counts after three identical ones. It comes straight from the
  v0.4.5 dogfood, where the model repeated `repo_search` several times in one
  turn.
- **A local reliability ledger** for tools, MCP and channels (#1438) — nothing
  leaves the machine.
- **`repo_search` fails fast** with no active repository (#1380).
- **`garraia stop` no longer treats a zombie as a live process** (#1426).

## New surfaces

- **The gateway is now an MCP server over HTTP, at `POST /mcp`** (#1513,
  Streamable HTTP) — alongside the existing stdio `garra mcp-server`.
- **`garra mcp-server` delivers its tools to Claude Code again** (#1518): since
  the `2026-07-28` spec (SEP-2549), `tools/list` requires `ttlMs` and
  `cacheScope`.
- **Memory and ledger retention: the data slice** (#1436) —
  `garraia-config::retention` + `garraia-db::retention`.
- **Effective mode per session** (#1409) and an **execution-profile badge** in
  the console header (#1410).
- **`search` mode can see MCP `filesystem` reads** (#1384) — write, edit and
  delete stay denied.

## Diagnostics: a clean install no longer starts yellow

- `/api/diagnostics` distinguishes **"never configured"** from **"broken"**
  (#1437): missing TLS, no `.env`, Telegram without a token or WhatsApp with no
  linked device stop going yellow on a fresh install.
- And the last two spurious warnings are gone (#1471): `tools.bash` with
  `agent.sandbox.mode = off` — the documented default of the `standard` profile
  — and `runtime.channels` with no messaging channel both move from `warning` to
  `not_configured`, with a correct remediation step. A sandbox that **is
  configured and unusable** stays a `warning`.

## Process and tooling

- **A mandatory D1–D10 dogfood gate before every tag**
  ([`docs/releasing.md`](https://github.com/michelbr84/GarraRUST/blob/main/docs/releasing.md) §1.5,
  #1439). v0.4.4 and v0.4.5 shipped green and broke on the real path; CI proves
  the code compiles, not that a clean install works for a person. The matrix is
  run **against the candidate**, with date, OS and operator recorded. **A row
  without a date means the release does not ship.**
- **The Linux part of D1/D9 is automated** (#1426):
  `scripts/dogfood/linux-clean-install.sh` installs the `.deb` into a bare
  `ubuntu:24.04`, runs the doctors, points at a local Ollama, requires a **real
  model reply** over REST, restarts, and proves that session and config
  survived.
- **CI brings MinIO up without a registry** (#1458). On 2026-09-24 `quay.io`
  started requiring a login for every `minio/minio` tag — after Docker Hub
  removed the repository and `dl.min.io` began answering 410. What is still
  public is the **source**: `scripts/ci/build-minio-image.sh` builds the pinned
  upstream tag (refusing if it no longer points at the expected commit) and tags
  the image the testcontainer looks for.
- The `pre-tool-use` hook no longer blocks `rm -rf /tmp/…` or `rm -rf ./x`: the
  block is anchored on the **target** (root, home, current directory, parent),
  not on the prefix (#1453).
- `scripts/setup-toolchain.sh` pins the toolchain locally via `rustup override`,
  with no `rust-toolchain.toml` — which breaks CI cross-compiles (#1452).
- iMessage: the room field is named `room_id` (#1461).

## Upgrading from v0.4.5

```bash
garraia update
```

No format changes: `config.yml`, the WhatsApp `session.enc`, `allow`/`owners`
and your history all keep working. Three visible behavior changes:

- Remote sessions with no project now keep files under
  `<data_dir>/workspace/<session>` — a new, empty folder per session. If you
  already declared `agent.file_roots`, nothing changes.
- Clients that used `X-Session-Id` (or the id in the URL) to **reach a channel
  session** through the API — writing to it, listing it or reading its history —
  now get `404`. That is exactly the path #1462 closes. The conversation is
  still reachable through its own channel, and an operator reads any session via
  `GET /admin/api/sessions/{id}/history`.
- An MCP server no longer registers slash commands (#1386). If you called an MCP
  tool as `/name`, call it as a tool instead, under the mode's policy.

## Known limitations

- The WhatsApp QR code is still **in the terminal**; moving it into the desktop
  app is [plan 0363](https://github.com/michelbr84/GarraRUST/blob/main/plans/0363-desktop-whatsapp-sem-terminal.md),
  not this release.
- The **global** Agents & Permissions page (#1433), permission **presets**
  (#1434) and policy **import/export** (#1435) are deferred to the next cycle:
  v2 proved the pattern on one page, and generalizing it is a scope decision.
- Windows installers and desktop bundles ship **unsigned** — the project has no
  code-signing certificate and no Apple Developer ID.
- The Tauri desktop auto-updater remains disabled by design; the supported
  update path is the CLI's `garra update`
  ([runbook](https://github.com/michelbr84/GarraRUST/blob/main/docs/releasing.md)).
