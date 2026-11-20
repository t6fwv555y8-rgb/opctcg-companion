# ChatGPT Skill System Design

**Evidence base (read this first).** This design comes from reading two local repos: `opctcg-companion` (45 commits) and `astra-command` including `company-os/` (14 commits), plus their docs and the `CURSOR_NEXT_PROMPT.md` file. I did **not** have your ChatGPT conversation history, Gmail, Calendar, or Drive. Anything about those is marked *assumed*. Section 8 lists what I need from you to close those gaps.

---

## 1. Map of recurring AI workflows (observed)

| # | Workflow | Evidence |
|---|---|---|
| A | **Milestone-driven agentic coding.** ChatGPT writes a long, numbered prompt for Cursor; Cursor builds; a "milestone report" comes back. | `CURSOR_NEXT_PROMPT.md` (goal, numbered scope, "do NOT add…", quality gate, "finish with a concise milestone report"); `docs/MILESTONE_{1,3,5,7,8,9}.md`; Cursor branch `cursor/milestone-6-live-validation-4c31` |
| B | **Cloud agent cannot validate live, so you do it on your Mac.** Documents say "only you can mark Live Validation: PASS". | `LIVE_VALIDATION.md`, `MAC_RUN.md` |
| C | **Mac desktop build, sign, notarize, DMG.** Recurring pain. | `MAC_BUILD/RELEASE/SIGNING`, `DMG_TROUBLESHOOTING`, `scripts/mac-*.js`, commits on folder picker and app bundle |
| D | **Security / authority / consent design** for agent actions: default-deny, approval fingerprints, HMAC channel, replay protection, EXTERNAL WRITES switch. | `WRITE_AUTHORITY`, `DESKTOP_AUTHORITY`, `PRIVILEGED_CHANNEL`, `IDEMPOTENCY`, `EXTERNAL_*` |
| E | **Prompt and schema engineering for business skills** (ICP Builder, Prospect Research, Outreach Writer, Content Ideas, Proposal Builder) with Company Brain context, Zod, and artifacts. | `company-os/README.md`; commit "State list field types in the … prompts" |
| F | **Domain-specific coaching/strategy logic** (OPTCG rules, combat math, beam search/MCTS, LLM coach grounded on GameState, event schema, backpressure). | `crates/optcg_*`, coach commits |
| G | **Data sync with politeness/privacy rules** (cache, spaced requests, never store player names, never save the opponent's hand). | `sync_cards.py`, `sync_meta.py`, README |
| H | **Hard debugging in Rust/tokio/Tauri** (self-deadlock on `parking_lot::Mutex`, diagnosed with `sample <pid>`). | `AGENTS.md` sharp edges |
| I | **Project memory upkeep** (`AGENTS.md`/`CLAUDE.md` pruning rules). | `AGENTS.md` "Maintaining this file" |
| J | Spreadsheet/visual deliverable (project bulletin board `.xlsx` with previews). | `outputs/project-bulletin-board/` (one-off) |
| K | *Assumed, not seen:* research, competitive/market analysis, decks/docs, Gmail/Calendar/Drive work. | Company OS targets sales and marketing work, so some of this is likely |

**Style and quality preferences visible in your repos.** These should be enforced, not re-explained:

- Commit messages are single imperative sentences with no prefix ("Pace the dev mock stream slower than…").
- Honesty about state: a demo mode is "clearly labeled", the UI "may display only runtime activity the backend can prove", and "Do not treat [planned items] as available now".
- Explicit non-goals in every milestone prompt.
- Docs are written for the Mac user, with the exact command to run and the exact thing you should see ("a window titled OPTCG Companion HUD", "a tab on localhost:1420 alone means…").
- Next.js in `company-os` is a version with breaking changes: read `node_modules/next/dist/docs/` first.

---

## 2. Recommended architecture

```
                        ┌────────────────────────────┐
 Orchestration          │  milestone-cycle           │
                        └─────────────┬──────────────┘
        ┌──────────────┬──────────────┼───────────────┬────────────────┐
 Universal       milestone-prompt-   mac-validation-   (research-with-   (decision-memo)
                 writer              and-release       sources) P1       P2
 QC              milestone-report-auditor  ·  authority-model-review (P1)
 Project-specific   optcg-companion-knowledge · mijo-company-os-skills · tauri-rust-debugging
```

Principles applied:

- Durable rules go in skills. Fast-changing state (current milestone, branch names, open bugs) is passed in by the user or read from `AGENTS.md` and docs at run time.
- Anything already in `AGENTS.md` stays there. The skills point to it and don't duplicate it.

---

## 3. Prioritized table

| Pri | Skill | Type | One-line value |
|---|---|---|---|
| **P0** | `milestone-prompt-writer` | Universal | Produces Cursor/Codex-ready milestone prompts in your exact house format |
| **P0** | `milestone-report-auditor` | QC | Checks an agent's "milestone complete" report against the diff, tests, and your honesty rules |
| **P0** | `mac-validation-and-release` | Universal (desktop apps) | Build/sign/notarize/DMG troubleshooting plus live-validation checklists |
| **P0** | `milestone-cycle` | Orchestration | Runs plan → prompt → audit → Mac validation → docs update as one loop |
| P1 | `authority-model-review` | QC/Project (Mijo) | Reviews any new agent capability against default-deny, approval, and replay rules |
| P1 | `mijo-company-os-skills` | Project | Authoring and testing the business skills (prompts, Zod schemas, Brain usage) |
| P1 | `optcg-companion-knowledge` | Project | Rules and event vocabulary, coach grounding, data-sync policy |
| P1 | `tauri-rust-debugging` | Universal (your stack) | Diagnosis playbook: deadlocks, sidecar hangs, async runtime stalls |
| P1 | `research-with-sources` | Universal | Sourced, verified research (*assumed need*; confirm) |
| P2 | `decision-memo` | Universal | Options → recommendation docs (*assumed need*; confirm) |

---

## 4. P0 specifications

### 4.1 `milestone-prompt-writer`

- **Purpose.** Turn "what's next" into a prompt a coding agent can execute without clarifying questions, in the structure your `CURSOR_NEXT_PROMPT.md` already uses.
- **Trigger.** "Write the next Cursor prompt for the Gmail draft feature." "Prompt for Milestone 10." "Turn this bug into a Codex task." "Cursor said it can't validate live. Write a follow-up."
- **Inputs.** Goal; current state (repo `AGENTS.md`/README/latest `docs/MILESTONE_*.md`, last commits); what's explicitly out of scope; known risks.
- **Outputs.** One pasteable prompt (single code block), plus a short "assumptions I made" list.
- **Workflow.**
  1. Read repo state (files, or ask for the last milestone doc and `git log`).
  2. Restate the **core law(s)** to preserve (e.g. "UI may display only backend-proven activity"; "default deny").
  3. Write the sections in this order:
     - Role line ("Act as the senior architect for X…")
     - Preconditions ("Milestone N complete only if tests pass…")
     - Goal
     - Numbered implementation list (≤12, each independently verifiable)
     - Explicit **Do not** list (your prompts always carry one)
     - Quality gate (observable, testable statements)
     - Required final report format (files changed, commands run, test results, remaining risks, ready for live Mac validation yes/no)
  4. Convert every vague verb ("support", "improve") into a testable behavior.
  5. Add a "what you cannot verify from the cloud" clause so the agent doesn't claim live validation.
- **Tools.** Files (read repo context). No web unless an external API's current behavior matters.
- **Bundled resources.**
  - `assets/milestone-prompt-template.md`
  - `references/house-rules.md` (honesty rules, commit style, non-goal patterns, drawn from your two repos)
  - `references/examples/` (the M2 prompt, trimmed; add 1–2 more later)
- **Scripts.** None. Linting is checklist-level.
- **Quality checks.** Every numbered item has a matching gate line. Non-goals present. Report format present. No requirement depends on something the agent can't observe. Under ~1 page of numbered scope per milestone. If bigger, split it.
- **Composition.** Called by `milestone-cycle`. Output later checked by `milestone-report-auditor`.
- **Example prompts.**
  1. "Write the next Cursor prompt: add Slack read-only connector to Mijo, following how GitHub read-only was done."
  2. "Cursor prompt to fix the DMG picker stacking bug, with a regression test."
  3. "Turn the five Company OS skills into a Milestone 5 prompt for workflow evals."

### 4.2 `milestone-report-auditor`

- **Purpose.** Stop "done" from meaning "claimed done". Cross-check an agent's final report against evidence.
- **Trigger.** "Cursor says M9 is complete, check it." "Here's the report and the diff." "Is this ready for Mac validation?"
- **Inputs.** The report; the original prompt; `git diff`/commit list; test output; relevant docs.
- **Outputs.** Verdict (Ready / Ready with caveats / Not ready), a requirement-by-requirement table (met, partially met, unverified, violated), non-goal violations, doc drift, and a **follow-up prompt** for the agent.
- **Workflow.**
  1. Extract each numbered requirement and gate line from the original prompt.
  2. For each, find evidence: a test name, a file, a commit. "Claimed but no evidence" = unverified.
  3. Scan for scope creep against the Do-not list (e.g. a Gmail *send* when only *draft* was allowed).
  4. Check honesty rules: planned features not described as shipped; demo/mock paths labeled; secrets never returned to the renderer; docs updated and consistent with code (`README` milestone status, `docs/*`).
  5. Separate what only a human on a Mac can confirm → hand to `mac-validation-and-release`.
  6. Emit a follow-up prompt for the gaps.
- **Tools.** Files, code execution (run the test commands the repo documents), `git`.
- **Resources.**
  - `references/audit-rubric.md`
  - `assets/audit-report-template.md`
  - `scripts/extract_requirements.py` (parses numbered list and quality-gate bullets from a prompt into a checklist JSON; deterministic, so no rewriting each time).
- **Quality checks.** No requirement left without a status. Verdict consistent with the table. Every "unverified" has either a command to verify or a human step.
- **Composition.** Follows `milestone-prompt-writer`; feeds `authority-model-review` when the milestone touches permissions.
- **Example prompts.**
  1. "Here's Cursor's M9 report and diff. Audit it."
  2. "Did this PR violate 'no external writes' in the milestone prompt?"
  3. "Audit and give me the follow-up prompt only."

### 4.3 `mac-validation-and-release`

- **Purpose.** Make the Mac-side human steps fast and repeatable: building, running, signing/notarizing, DMG problems, and live validation packets.
- **Trigger.** "Give me the live validation checklist for this milestone." "Notarization failed with…" "App opens a browser tab, not the HUD." "DMG won't open."
- **Inputs.** Project, milestone, platform notes, error output or screenshots.
- **Outputs.** Either (a) a validation checklist with exact commands and **expected visible result for each step** plus a PASS/FAIL results table for you to fill in, or (b) a diagnosis with ordered fixes.
- **Workflow.**
  1. Identify the mode: run, build, sign, notarize, DMG, or live validation.
  2. For validation: derive steps from the milestone's acceptance gates, and mark each step *human-only* vs *scriptable*.
  3. Always distinguish "frontend dev server in a browser" from "native app window" (a trap in both your projects).
  4. For release errors: match against known failure signatures first (`references/known-failures.md`), then ask for the one specific log excerpt needed.
  5. After the run, turn your results table into a short status update and open issues.
- **Tools.** Files; browser only to check current Apple notarization docs when the error is unfamiliar.
- **Resources.**
  - `references/known-failures.md` (seeded from `DMG_TROUBLESHOOTING`, `MAC_SIGNING`, picker/app-bundle commits)
  - `assets/live-validation-template.md`
  - `scripts/preflight_mac.sh` (checks Xcode CLT, Rust toolchain version, Node version, signing identity presence, and that the notary keychain profile exists; prints pass/fail. Deterministic, so a script not prose.)
- **Quality checks.** Every step has a command **and** an observable outcome. No step asks you to paste secrets. Release steps respect the "your own Apple credentials" boundary.
- **Composition.** Handoff target from `milestone-report-auditor`; a stage in `milestone-cycle`.
- **Example prompts.**
  1. "Live validation checklist for the approvals flow in Mijo."
  2. "notarytool returned Invalid, here's the log."
  3. "OPTCG HUD shows SYNCING forever on OneSimulator. What do I check?"

### 4.4 `milestone-cycle` (orchestrator)

- **Purpose.** Run the whole loop so each turn knows the stage.
- **Trigger.** "Let's do the next milestone for Mijo." "What stage are we in?" "Continue the cycle."
- **Workflow.**
  1. **Orient.** Read `AGENTS.md`, latest milestone doc, README status. Output a 5-line current state.
  2. **Scope.** Propose the next milestone (≤1 goal, ≤12 items, explicit non-goals). Wait for approval.
  3. **Prompt** → `milestone-prompt-writer`.
  4. **Audit** → `milestone-report-auditor` when you paste the report.
  5. **Validate** → `mac-validation-and-release`.
  6. **Close.** Draft the `docs/MILESTONE_N.md` update, README status line, and one-sentence commit messages in your house style. Propose `AGENTS.md` pruning only if something is durable and non-obvious.
- **Resources.** `references/stage-checklist.md`; `assets/milestone-doc-template.md`.
- **Quality checks.** The stage is always stated. Nothing is called complete before audit **and** human validation (or an explicit "not live-validated" label).
- **Example prompts.** "Start the next milestone for OPTCG Companion." "Cursor finished. What now?"

---

## 5. P1/P2 specifications (short)

**`authority-model-review` (P1).** Reviews a proposed capability or diff against Mijo's trust model: default deny, per-action approval with payload fingerprint and expiry, desktop-origin signing, replay protection, authority reset on launch and credential change, never returning secrets to the renderer, and idempotency. Output: a threat table and required tests. Resources: `references/trust-model.md` (condensed from `DESKTOP_AUTHORITY`, `PRIVILEGED_CHANNEL`, `WRITE_AUTHORITY`, `IDEMPOTENCY`; **link to docs, don't copy volatile detail**). Trigger: "Can I let Mijo send a Slack message?" Composes with the auditor.

**`mijo-company-os-skills` (P1).** For building or improving the five business skills: Company Brain fields, Zod output schemas, **explicit list field types in prompts**, what "Prepare" autonomy means (drafts only), refusal guards, run snapshot, and eval fixtures (good/bad outputs). Resources: `references/skill-spec-template.md`, `references/brain-schema-notes.md`, `scripts/validate_skill_output.mjs` (runs a sample output through the Zod schema). Note: Next.js here has breaking changes, so read the in-repo docs first.

**`optcg-companion-knowledge` (P1).** Event vocabulary (pipe-delimited and JSON formats), GameState concepts, combat math terms, coach grounding rules, data-sync policy (cache, spaced requests, no player names, no opponent hand), platform matrix (Mac = OneSimulator; OPTCGSim capture is Windows-only). Resources: `references/events.md`, `references/data-policy.md`. Keep card rules out; they live in the DB.

**`tauri-rust-debugging` (P1).** Playbook seeded from the `parking_lot` self-deadlock: symptoms → `sample <pid>` → look for `RawMutex::lock_slow` → fix pattern (lock once, copy to locals) → OS-thread-bounded regression test. Add the sidecar-won't-start and "UI shows nothing" checklists. Resource: `references/playbook.md`. Script: `scripts/sample_hangs.sh` (wraps `sample` and greps for lock_slow).

**`research-with-sources` (P1, assumed).** Multi-source research with citation per claim, contradiction notes, and a confidence label. Build only if you confirm you use ChatGPT for research and competitive work.

**`decision-memo` (P2, assumed).** Options, criteria, recommendation, risks, a reversal trigger. Build later if you confirm the need.

---

## 6. Composition map

```
milestone-cycle
 ├─ (orient) AGENTS.md + docs
 ├─ milestone-prompt-writer ──▶ [coding agent runs]
 ├─ milestone-report-auditor ──▶ authority-model-review   (if permissions touched)
 │                           ──▶ tauri-rust-debugging     (if hangs/deadlocks reported)
 ├─ mac-validation-and-release
 └─ (close) milestone doc + README + commit messages

Project knowledge, loaded by whichever skill needs it:
  mijo-company-os-skills · optcg-companion-knowledge
Optional upstream: research-with-sources → decision-memo → milestone-cycle (scope)
```

---

## 7. Considered and rejected

| Candidate | Why not |
|---|---|
| Generic code-review / QA skill | Standard ChatGPT does this well. The auditor encodes only your report-vs-evidence loop. |
| Spreadsheet / xlsx skill | One-off output seen; built-in capabilities suffice. |
| Presentation/deck skill | No decks in either repo. Add only if you confirm a need. |
| Gmail/Calendar/Drive skills | Nothing in the repos shows a repeated workflow. Mijo *builds* Gmail drafting; that's product code, not your ChatGPT usage. |
| Per-milestone skills | Fast-changing state. Belongs in `docs/MILESTONE_N.md`. |
| Duplicating `AGENTS.md` into a skill | Drifts; reference it instead. |
| Card database / rules-text skill | Large, changing data; keep in the DB and sync scripts. |
| Separate prompt-writer per project | Same format; project differences go in references. |
| Image/design generation skill | No design-generation workflow observed. |

---

## 8. Information I still need

1. Do you use ChatGPT for **research, competitive analysis, decks, docs, Gmail, Calendar, Drive**? If so, which are weekly?
2. Other projects I haven't seen (the prompt says "all of these projects").
3. Which coding agent receives your prompts now (Cursor, Codex, Claude Code)? Format differences matter.
4. Do you want the milestone prompt written for **one** agent or portable?
5. Two or three of your best past milestone prompts and reports, plus one that went badly (to improve the rubric).
6. Anything ChatGPT keeps getting wrong or you keep re-explaining that isn't in the repos.
7. Windows needs? (Mijo is "Windows later"; OPTCG has a Windows capture path.)
8. Are the Company OS skills for a real customer, or an internal product? That affects eval rigor.

---

## 9. Build order

1. `milestone-prompt-writer`: highest frequency, template already exists.
2. `milestone-report-auditor`: biggest quality lever.
3. `mac-validation-and-release`: seed from existing docs and known failures.
4. `milestone-cycle`: glue, once the three above are stable.
5. `authority-model-review`, then `tauri-rust-debugging`.
6. `mijo-company-os-skills`, `optcg-companion-knowledge`.
7. Research/decision skills only after Section 8 answers.

---

# HANDOFF TO CHATGPT

You are building a set of ChatGPT Skills for one user who builds local-first desktop apps with AI coding agents. Do **not** merge them into one skill. Each is a directory with `SKILL.md` (concise control plane, front-matter with `name` and a `description` stating what it does **and** when to trigger) and optional `references/`, `scripts/`, `assets/`. Keep generic advice out. Put long material in `references/`.

**What I learned about the user's work.**
- Two active projects:
  - **OPTCG Companion.** Rust workspace (`optcg_core/database/rules/coach/events/observation`), Tauri backend, React+Tailwind HUD, Python scripts (`mock_stream.py`, `sync_cards.py`, `sync_meta.py`), WebSocket port 9002, an LLM "coach" grounded on GameState, a browser extension for OneSimulator.
  - **Mijo Commands (repo `astra-command`).** Tauri 2 shell + Node sidecar, signed HMAC desktop-command channel, approval-gated external writes (GitHub issue create is the only one), macOS Keychain credentials. Includes **Company OS**: Next.js, SQLite/Drizzle, Zod, Company Brain, five "Prepare-only" business skills.
- The workflow is: ChatGPT drafts a long milestone prompt → a coding agent (Cursor) builds → a milestone report returns → the user validates live on a Mac, since cloud agents cannot.
- House conventions:
  - Prompts have a role line, preconditions, goal, ≤12 numbered items, an explicit Do-not list, a quality gate, and a required final-report format.
  - Commit messages are single imperative sentences.
  - Docs give the exact command and the exact thing to see.
  - Never describe planned features as shipped, and label demo/mock modes.
  - Secrets never reach the renderer; privileged actions need signed desktop approval and default to deny.
  - `AGENTS.md` holds durable sharp-edges only and is pruned rather than appended to.

**Create these skills, in this order.**

1. **`milestone-prompt-writer` (P0).**
   - Description: "Writes coding-agent milestone prompts in the user's house format. Use when the user asks for the next Cursor/Codex prompt, a milestone prompt, or to turn a bug/feature into an agent task."
   - Files: `assets/milestone-prompt-template.md`, `references/house-rules.md`, `references/examples/m2-desktop-authority.md`.
   - Workflow: read repo state, state the laws to preserve, fill the template, make every item testable, add the cloud-can't-validate-live clause, list assumptions.
2. **`milestone-report-auditor` (P0).**
   - Description: "Audits a coding agent's milestone-complete report against the original prompt, diff, and tests. Use when the user pastes a report/PR or asks if a milestone is done."
   - Files: `references/audit-rubric.md`, `assets/audit-report-template.md`, `scripts/extract_requirements.py` (numbered items and gate bullets → checklist JSON).
   - Output: verdict, requirement table (met/partial/unverified/violated), non-goal violations, doc drift, follow-up prompt.
3. **`mac-validation-and-release` (P0).**
   - Description: "Builds Mac validation checklists and diagnoses build/sign/notarize/DMG/run problems for Tauri apps. Use for live-validation requests or Apple signing errors."
   - Files: `references/known-failures.md`, `assets/live-validation-template.md`, `scripts/preflight_mac.sh`.
   - Rule: each step = command + observable result; distinguish native window from browser dev server; never ask for secrets.
4. **`milestone-cycle` (P0, orchestrator).**
   - Description: "Runs the orient → scope → prompt → audit → validate → close loop. Use when starting, continuing, or closing a milestone."
   - Files: `references/stage-checklist.md`, `assets/milestone-doc-template.md`. Always state the current stage; never call work complete without audit and human validation.
5. **`authority-model-review` (P1).** Reviews new agent capabilities against default-deny, fingerprinted per-action approval with expiry, signed desktop origin, replay protection, authority reset, secret handling, idempotency. Output: threat table + required tests. Reference docs by path; don't copy them.
6. **`tauri-rust-debugging` (P1).** Playbook with the `parking_lot` double-lock deadlock as the seed case (`sample <pid>`, `RawMutex::lock_slow`, lock-once fix, OS-thread-bounded regression test), plus sidecar and async-stall checklists. Script `scripts/sample_hangs.sh`.
7. **`mijo-company-os-skills` (P1).** Authoring/testing the five Prepare-only skills: Brain fields, Zod schemas, explicit list field types in prompts, refusal guards, run snapshots, eval fixtures. Script `scripts/validate_skill_output.mjs`. Remind: the Next.js version has breaking changes; read `node_modules/next/dist/docs/` first.
8. **`optcg-companion-knowledge` (P1).** `references/events.md` (pipe and JSON event formats), `references/data-policy.md` (cache, spaced requests, no player names, no opponent hand, Mac = OneSimulator only), coach grounding rules.
9. **Hold** `research-with-sources` and `decision-memo` until the user confirms they use ChatGPT for research and memos.

**Constraints.**
- Skills hold durable procedures and knowledge only. Current milestone, branch names, and open bugs come from the user or from repo docs at run time.
- Don't duplicate `AGENTS.md`; link to it.
- Use scripts for deterministic work (requirement extraction, Mac preflight, schema validation, hang sampling).
- Each `SKILL.md` stays short. Include "Quality checks before declaring done" and "Composition" sections.
- Before building, ask the user the unanswered questions in Section 8 (especially: which coding agent they use, and whether they use ChatGPT for research, decks, Gmail, Calendar, or Drive).
