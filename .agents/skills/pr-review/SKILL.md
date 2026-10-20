---
name: pr-review
description: Review a pull request, or your own branch before opening one, on three separate axes (Correctness, Standards, Spec) run as parallel sub-agents. Use when asked to review a PR, a branch, or changes before opening a PR.
---

Three-axis review of one change:

- **Correctness**: does the code work? Logic errors, panics, ignored errors, untested behaviour.
- **Standards**: does the code follow [`CODING_STANDARDS.md`](../../../CODING_STANDARDS.md)?
- **Spec**: does the code do what the issue or spec asked for, and nothing else?

Each axis runs in its own sub-agent and owns its own sections of `CODING_STANDARDS.md`,
so every finding is reported once, by the axis that owns it.

The review is read-only: it never edits files, formats code, checks out branches or posts to GitHub.

**Input**:

- A PR number or URL (`/pr-review 4363`) → **PR mode**.
- Nothing, or a base ref (`/pr-review`, `/pr-review origin/release-1.5`) → **local mode**, for reviewing your own branch before opening a PR. The base defaults to `main` on the remote for `thin-edge/thin-edge.io` (often `upstream` when `origin` is a fork).

## Process

### 1. Pin the change

Create a scratch directory for this review, and write the diff to `diff.patch` and the commit list (`--format='%H%n%B'`, so sign-offs are visible) to `commits.txt` inside it.
In PR mode, also write the PR title and body to `pr-body.md`.

**PR mode**:

```sh
gh pr view <N> --json number,url,title,body,baseRefName,headRefOid,commits,closingIssuesReferences
gh pr diff <N> > diff.patch
git fetch <remote> pull/<N>/head <baseRefName>
git log <remote>/<baseRefName>..<headRefOid>
```

`<remote>` is the remote whose URL matches the PR's base repository (from `url`).
It is often `upstream`, because `origin` may be a fork that has no `pull/<N>/head` ref.

Sub-agents read the changed code at the PR's head with `git show <headRefOid>:<path>`, because the working tree is a different branch.

**Local mode**:

```sh
git diff $(git merge-base <base> HEAD)        # committed and uncommitted changes
git ls-files --others --exclude-standard      # new files the diff does not show
git log <base>..HEAD
```

Sub-agents read files from the working tree, and must also read each untracked file in full.
If `gh pr view` finds an open PR for this branch, say so in one line and carry on in local mode.

In both modes, `CODING_STANDARDS.md` and the skills are read from the working tree:
the review applies your checkout's standards, whichever version the change was written against.

The step is done when the ref resolves, the diff (plus untracked files) is non-empty, and `diff.patch` is a raw patch that starts with `diff --git`
(shell wrappers that summarise `git` output can corrupt it; rerun the command without the wrapper).
A bad ref or empty change stops the review here, before any sub-agent starts.

### 2. Find the spec

Collect every source that says what the change should do:

An issue reference is a `#NNN` or a full GitHub issue URL; this repo forbids auto-close keywords, so `closingIssuesReferences` is usually empty and the URL form is common.
Fetch each with `gh issue view`. Read only the issues the change references, not the issues they reference in turn.

- **PR mode**: the PR title and body, every issue in `closingIssuesReferences`, and every issue referenced in the body or commit messages.
- **Local mode**: issues referenced in commit messages or the branch name, or a spec path the user passed. If none is found, ask the user once where the spec is; "there isn't one" is a valid answer.
- **Both**: any `openspec/changes/<name>/` directory the diff touches, or whose name matches the branch. Read its proposal, design, specs and tasks.

With no spec, the Spec axis is skipped and the report says "No spec available".

### 3. Run the three axes in parallel

Send all sub-agent calls in one message, and wait for all three reports.
Every prompt includes: the scratch directory path, where to read files from (head SHA or working tree), and this sentence:
"Report only what your axis owns; the other axes cover the rest. Give each finding a severity (`[critical]`, `[warning]` or `[note]`), a `file:line` and a suggested fix. Under 400 words."

**Correctness** prompt:

- "Follow `.agents/skills/rust-review/SKILL.md` in correctness-only mode on the diff in `diff.patch`."
- Owned sections of `CODING_STANDARDS.md`: **Correctness** (except the **Errors** rules on `thiserror`/`anyhow` and on message wording, which Standards owns), **thin-edge.io specifics**, and from **Tests** the subsections **What to test** and **Speed and determinism**.
- "Trace every changed branch with a concrete input. Every changed behaviour either has a named test that covers it or a finding saying which test is missing. A missing-test finding names a concrete regression that the existing tests would not catch; if you cannot name one, it is not a finding."

**Standards** prompt:

- Owned sections of `CODING_STANDARDS.md`: **Toolchain and formatting** (only what `rustfmt` and `clippy` do not already catch), from **Errors** the rules on `thiserror`/`anyhow` and on error message wording, **Paths and logging**, from **Tests** the subsections **Naming and layout** and **Tools**, **Idiomatic Rust**, **Comments and documentation**, and **Commits and pull requests** (check every commit in `commits.txt`, and `pr-body.md` in PR mode).
- Any other file the repo has about how to write code, such as `CODING_GUIDELINES.md` or `CONTRIBUTING.md`; where it conflicts with `CODING_STANDARDS.md`, `CODING_STANDARDS.md` wins.
- The [smell baseline](#smell-baseline) below, pasted in full.
- "Cite the rule (file and section) for each documented-standard breach. Report smells as judgement calls ('possible Feature Envy'), never as breaches, and drop a smell where a documented standard endorses the code."
- "Report findings in two lists. **Findings**: breaches of untagged rules, Idiomatic Rust suggestions whose fix has the payoff described at the top of that section (state the payoff), and smells the smell baseline classes as Findings. **Polish**: breaches of rules tagged (polish), and every other Idiomatic Rust suggestion or smell. A documented rule is polish only if it carries the (polish) tag, or sits in the Idiomatic Rust section; a breach of any other rule is a Finding, however small. Only Findings count towards the 400-word limit; keep each Polish item to one line."

**Spec** prompt (only when a spec was found):

- The spec contents, or the paths to read.
- "Report: (a) requirements the spec asks for that are missing or partial; (b) behaviour in the diff that the spec did not ask for; (c) requirements that look implemented but where the implementation looks wrong. Quote the spec line for each finding."
- Spec also owns places where the PR description or a commit message says something the code does not do.
  When such a mismatch also leaves code untested, Correctness reports the missing test and Spec reports the mismatch.

### 4. Report

Present the three reports under `## Correctness`, `## Standards` and `## Spec`, verbatim or lightly cleaned.
Keep each axis's findings in its own section, in that axis's order.
A change can pass one axis and fail another (correct code that breaks conventions, or clean code that implements the wrong thing), and a single ranked list lets one axis hide the other.

The Standards Polish list depends on the mode:

- **PR mode**: leave it out, and add one line under Standards: "<n> polish items not listed (see CODING_STANDARDS.md)". Polish is not worth asking another author to change.
- **Local mode**: show it under a final `## Polish` heading. Before your own PR, everything is worth fixing.

End with:

- one line per axis: number of findings and the worst one (Polish items do not count);
- a verdict: **Approve**, **Request changes** or **Needs discussion**;
- in local mode, a reminder to run `just format` and `just check` before opening the PR;
- in PR mode, an offer to draft the findings as PR comments.

### 5. Draft PR comments

PR mode only, when the user takes up the offer or asks for comments.
Draft one comment per finding, following [Writing PR comments](#writing-pr-comments), and anchor each on a changed line.
Findings with no changed line to anchor on go in a draft review body.

Then give the drafts a **cold read**: one sub-agent, given only `diff.patch` and the drafts, reports for each draft every sentence it had to read twice and every term used before it was explained.
Tell it that it reads as a thin-edge.io maintainer who knows Rust and Unix but has seen only this diff, so it flags what this change's reader would miss, not standard library or system calls.
Rewrite the flagged sentences in the visible prose; flags inside `<details>` need no change.
Run at most two rounds: each round finds smaller things, so a third rarely pays.
The step is done after the second round, or sooner when a round flags nothing in the visible prose.
Show the user every draft with its `file:line`, and under each draft any flag from the last round that you did not fix.

## Writing PR comments

A comment should be understood in one read by someone who has seen the diff and nothing else.

- **Prefix.** Start with `[AI] ` and then the team's conventional-comment prefix:
  `issue:` for a defect, `suggestion:` for a better approach, `todo:` for a missing test or doc,
  and `question:` when the finding depends on what the author intended.
  When you are unsure whether something is wrong, ask: a question costs the author little if the answer is "yes, on purpose".
- **Why, not what.** The reader can see the code.
  Write what goes wrong for the user or the system, and why.
  Describe the code only where the failure depends on a detail the reader would miss.
- **The reader's level.** Use only ideas that appear in the diff or the PR description.
  When the failure depends on another subsystem, explain that idea in one clause before using it.
- **Plain English.** One idea per sentence, under 20 words each, active voice, common words, no idioms.
- **Short.** Keep the prose under 80 words.
  Put a reproduction or a complete new test in `<details><summary>…</summary>`, so the comment reads at a glance.
- **Output, not claims.** When you ran something, paste the command and the relevant lines of its output.
  "Verified" or "confirmed" appears only next to that output.
- **Smallest fix.** Give the fix as a `suggestion` block with the minimal change, or in one sentence when it spans several places.
  A new test belongs in a comment only when the finding is that the behaviour is untested.
- **Existing problems.** When the problem is already on `main`, say so and offer it as a follow-up.
- **One line per paragraph.** GitHub shows every line break in a comment, so write each paragraph on a single line,
  with a blank line between the prefixed headline and the paragraphs that follow.

```markdown
[AI] question: Does the action need to run in `tmp.path`?

If that directory is missing, the spawn fails and the user sees "Service command not found", so they look in `system.toml` instead. If the action does not need the tmp dir, keep the caller's directory. If it does, name the directory in the error.
```

## Smell baseline

A fixed set of Fowler code smells (_Refactoring_, ch.3), checked against the diff even where the repo documents nothing.
Each is a heuristic: report it as "possible <smell>", and drop it where a documented repo standard endorses the code.
A smell is a symptom: before classifying one, ask what design problem would produce it.
A smell is a **Finding** when:

- the change works around a design that no longer fits, where the justified fix is a larger refactoring the change avoided
  (a flag threaded through several layers, a special case bolted onto a general path, a type stretched to hold a concept it was not built for);
  name the refactoring and say why the change justifies it;
- or its fix has the payoff described at the top of the Idiomatic Rust section in `CODING_STANDARDS.md`.

Otherwise it is **Polish**: a local blemish whose fix would only tidy the code.

Each smell reads *what it is* → *how to fix*:

- **Mysterious Name**: a function, variable, or type whose name doesn't reveal what it does or holds. → rename it; if no honest name comes, the design is unclear.
- **Duplicated Code**: the same logic shape appears in more than one hunk or file in the change. → extract the shared shape, call it from both.
- **Feature Envy**: a function that reaches into another type's data more than its own. → move the function onto the data it uses.
- **Data Clumps**: the same few fields or parameters keep travelling together. → bundle them into one type, pass that.
- **Primitive Obsession**: a primitive or string standing in for a domain concept that deserves its own type. → give the concept its own small type.
- **Repeated Switches**: the same `match`/`if`-cascade on the same type recurs across the change. → move the behaviour onto the type, or share one lookup between both sites.
- **Shotgun Surgery**: one logical change forces scattered edits across many files in the diff. → gather what changes together into one module.
- **Divergent Change**: one file or module is edited for several unrelated reasons. → split so each module changes for one reason.
- **Speculative Generality**: abstraction, parameters, or hooks added for needs the spec doesn't have. → delete it; inline back until a real need shows.
- **Message Chains**: long `a.b().c().d()` navigation the caller shouldn't depend on. → hide the walk behind one method on the first value.
- **Middle Man**: a type or function that mostly just delegates onward. → cut it, call the real target directly.
- **Refused Bequest**: a trait implementation that ignores or stubs out most of what the trait asks for. → implement a narrower trait, or use composition.
