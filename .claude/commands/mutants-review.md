---
name: mutants-review
description: Run cargo-mutants on a PR's diff, verify a test for each missed mutant, and draft them as pending GitHub review comments
disable-model-invocation: true
argument-hint: "[PR number, defaults to the current branch's PR]"
---

Perform the review process defined here:

@.agents/skills/mutants-review/SKILL.md

Apply it to: $ARGUMENTS
