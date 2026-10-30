---
name: mutants-review
description: Run cargo-mutants on a PR's diff, verify a test for each missed mutant, and draft them as pending GitHub review comments
disable-model-invocation: true
---

# Mutation review of a PR

Finds code the PR changed that its tests do not pin down, writes a test that kills each **missed** mutant, proves the test does so, and drafts the tests as review comments. All work happens in a throwaway worktree of the PR head, so the user's checkout is never touched; the only output is a pending review the user submits themselves.

## 1. Pin the PR

1. Resolve the PR: `gh pr view <PR> --json number,url,headRefOid`, leaving out `<PR>` to use the current branch's PR.
2. Fetch the head from the remote whose URL matches the PR's base repo (from `url`; often `upstream`, since `origin` may be a fork): `git fetch <remote> pull/<number>/head`.
3. `git worktree add --detach /tmp/mutants-review-<number> <headRefOid>`. Run every later command from that directory.
4. `mkdir -p target/mutants-review && gh pr diff <number> > target/mutants-review/pr.diff`.

Done when the worktree is at `headRefOid` and holds the diff file.

## 2. Run cargo-mutants

List first to size the run:

```sh
cargo mutants --in-diff target/mutants-review/pr.diff --list
```

More than ~150 mutants takes a long time on this workspace: show the count per file and ask the user whether to narrow with `-f <glob>` before running.

Then run it in the background (it routinely exceeds the foreground timeout):

```sh
cargo mutants --in-diff target/mutants-review/pr.diff \
  --test-tool nextest --all-features -o target/mutants-review/run
```

`--test-tool nextest --all-features` mirrors `just test-unit`; plain `cargo test` misses feature-gated tests and reports false misses.

Results land in `target/mutants-review/run/mutants.out/`: `missed.txt` holds the targets; `timeout.txt` and `unviable.txt` are only reported, as counts.

cargo-mutants runs only the tests of the crate that holds each mutant, so a mutant in a library crate is reported missed even when a crate that calls it tests the behaviour. Rerun the missed mutants with those crates' tests added, using `-o target/mutants-review/downstream`, the `-F` filters from step 4, and one `--test-package` for the mutant's own crate and for each crate that calls the mutated function (use the `name` from each crate's `Cargo.toml`, which may contain hyphens, e.g. `tedge-agent`). Only mutants still missed after this go to step 3, and step 4 uses the same `--test-package` list.

Done when the runs have exited and the final `missed.txt` is read.

## 3. Give every missed mutant a verdict

For each line of `missed.txt`, read the mutated function and the tests that cover it, then assign one verdict:

- **Gap**: a real behaviour the PR introduced is untested. Write the smallest test that fails on the mutant and passes on the real code. Put it where that crate's existing tests for the function live, matching their style and helpers.
- **Equivalent**: the mutant cannot change observable behaviour (e.g. `>` vs `>=` where equality is unreachable, a `Default` that returns the same value). Record a one-sentence reason naming why it is unobservable.
- **Not worth pinning**: `Debug`/`Display` output or log messages, where a test would only freeze incidental text; or behaviour a test can only observe at prohibitive cost (e.g. waiting out a real timeout of tens of seconds). Record a one-sentence reason.

Test names describe the behaviour pinned, never the mutant (`rejects_port_zero`, not `kills_mutant_line_42`).

Done when every missed mutant has exactly one verdict.

## 4. Verify the gap tests

With all gap tests written into the tree, rerun cargo-mutants on just those mutants:

```sh
cargo mutants -f <mutated file> ... \
  --test-tool nextest --all-features -o target/mutants-review/verify \
  -F '<mutant line regex>' -F '<mutant line regex>' ...
```

`--in-diff` is unusable here: cargo-mutants rejects the diff once the new tests change the files it covers. Instead, `-f` scopes to the mutated files and each `-F` is a regex matching the mutant's full line from `missed.txt`, with regex metacharacters escaped. Keep the `path:line:col: ` prefix: names such as `delete match arm` are shared by several mutants in a file, and the prefix is what tells them apart. A test inserted above the mutated line shifts its line number; in that case take the new number from `cargo mutants -f <file> --list`.

This checks both halves at once: the baseline fails if a new test fails on the real code, and each targeted mutant must now appear in `caught.txt`. A mutant still in `missed.txt` means the test asserts the wrong thing; revise it and rerun. After two failed revisions, mark it **unverified** with the attempted test and an explanation of what it could not observe, and move on.

Capture each test's text and the file it belongs in.

Done when every gap test is either in `caught.txt` or explicitly marked unverified.

## 5. Draft the review

Show the user a summary table (mutant, verdict, verified?) and the comment bodies. Under it, name the changed logic no mutant touched: cargo-mutants leaves macro arguments (e.g. the arms of a `matches!`) and most `if` conditions unmutated, so "all caught" says nothing about them. Posting is outward-facing: post only after the user approves.

Each untested behaviour becomes one comment, anchored on a changed line of `pr.diff` that lies inside the mutated function, `side: RIGHT`. Mutants killed by the same test share one comment.

Write each comment following [Writing PR comments](../pr-review/SKILL.md#writing-pr-comments) in the pr-review skill. For a gap, that means:

- **Headline**: `[AI] todo: No unit test checks that <behaviour>.`, naming the behaviour in the user's terms (`tedge disconnect` removes the bridge), never the mutation.
- **Why it matters**: say which existing tests pass on the mutant, then what the user would see if that code broke. Describe the mutant in plain words ("if `stop_bridge` did nothing"), not in cargo-mutants terms ("replace `stop_bridge` with `Ok(())`").
- **The test**, in `<details><summary>Unit test that covers it (for <code>mod tests</code> in <code>file.rs</code>)</summary>`, indented to sit in that module. Above the code block, inside `<details>`, put the extra `use` lines it needs and any condition under which it does not bite (e.g. only on hosts with a binary on `PATH`).
- **Verification**, also inside `<details>`: the `cargo mutants` command from step 4 and its `caught` line for this mutant, or the unverified explanation.

Equivalent and not-worth-pinning verdicts go in the review body as a short list, not as line comments.

Then give the drafts the cold read from step 5 of the pr-review skill, and fix what it flags before showing them to the user.

Post them **pending**, so only the user sees them until they submit.

GitHub allows one pending review per user per PR, so look for one first:

```sh
gh api graphql -f query='query{repository(owner:"<owner>",name:"<repo>"){pullRequest(number:<number>){reviews(states:PENDING,first:5){nodes{id author{login} comments(first:50){nodes{path line body}}}}}}}'
```

- **One exists**: read its comments and match their style; drop any gap or note an existing comment already makes. Add each comment with `addPullRequestReviewThread(input:{pullRequestReviewId, path, line, side:RIGHT, body})`, passing the body with `-F body=@file` to keep its formatting intact.
- **None exists**: `gh api repos/<owner>/<repo>/pulls/<number>/reviews --input review.json`, with `review.json` holding `commit_id` (the `headRefOid`), `body`, and `comments: [{path, line, side, body}]`, and no `event` field (omitting `event` is what keeps it pending).

Done when the review reads back as `state: PENDING` with the new comments; give the user the comment URLs, then `git worktree remove --force /tmp/mutants-review-<number>`.
