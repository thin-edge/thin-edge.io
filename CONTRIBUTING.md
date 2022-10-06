# Contributing to thin-edge.io

Thanks for taking the time to contribute to thin-edge.io!

Contributing is not limited to writing code and submitting a PR. Feel free to
submit an [issue](https://github.com/thin-edge/thin-edge.io/issues) or comment
on an existing one to report a bug, provide feedback, or suggest a new feature.
You can also join us on GitHub Discussions.

Of course, contributing code is more than welcome! If you're planning to submit
a PR to implement a new feature or fix a bug, please open an issue that explains
the change and the motivation for it.

If you are interested in contributing documentation, please note the following:

- Doc issues are labeled with the `doc` label.
- The thin-edge.io docs content is in the `docs/src/` directory.

[How to build from source.](./docs/src/BUILDING.md)

<br/>
<br/>

# Pull request and git commit guidance

To assure a consistent level of quality and appearance, the project has some
rules for contributors to follow.

## Opening PRs and organizing commits

PRs should generally address only 1 issue at a time. If you need to fix two
bugs, open two separate PRs. This will keep the scope of your pull requests
smaller and allow them to be reviewed and merged more quickly.

When possible, fill out as much detail in the pull request template as is
reasonable. Most important is to reference the GitHub issue that you are
addressing with the PR.

**NOTE:** GitHub has [a feature](https://docs.github.com/en/github/managing-your-work-on-github/linking-a-pull-request-to-an-issue#linking-a-pull-request-to-an-issue-using-a-keyword)
that will automatically close issues referenced with a keyword (such as "Fixes")
by a PR or commit once the PR/commit is merged. Don't use these keywords. We
don't want issues to be automatically closed. We want our testers to
independently verify and close them.

## Writing good commit messages

Having clear, concise and well-written commit messages for all changes is not
only an indicator for the seriousness of the project, but also a way to ensure
sustainable development via replicability of the sources of the project.

Because of this, we try to ensure the highest possible quality for each
individual commit message.

Because such a thing cannot necessarily or in full be enforced via tooling, it
remains mainly the obligation of a reviewer of a change-set to ensure

* Atomicity of the individual commits
* that one commit is one atomic change, and not an accumulation of several
individual and separate changes
* that the commit message of the change expresses why the change was made

Every reviewer of a pull request is asked to review not only the changes, but
also the commit messages.
Reviewers are explicitly empowered and actually encouraged to block pull
requests that contain commit messages that do not meet a certain standard
even and especially also if the proposed changes are acknowledged.

The contributor should document why they implemented a change. A good
rule of thumb is that if a change is larger than 60 lines, a commit message
without a body won't suffice anymore.
The more lines are touched by an individual commit, the more lines should be
used to explain the change.


Also, some hard criteria are checked via github action lints:

* The `Signed-off-by` trailer lint must be present in all commits (also see
section "Ensuring the Signed-off-by-trailer")
* The commit body must meet the default formatting of a commit message, that
is
    * Subject line not longer than 50 characters
    * Subject capitalized
    * No period at the end of the subject
    * Imperative mood in the subject
    * Second line empty
    * Body lines not longer than 72 characters

You can also read about commit messages in
[this excellent guide](https://cbea.ms/git-commit/).

Commits using the `--fixup` or `--squash` feature of `git-commit` are allowed,
but will have to be squashed by the pull request author before being merged.

### Ensuring the Signed-off-by-trailer

Contributors need to sign the "Contributor License Agreement". They do so by
signing off each individual commit with a so-called "Signed-off-by Trailer".
Git offers the `--signoff`/`-s` flags when using `git-commit` to commit
changes.

To ensure that all commits have the trailer present in the commit message, a
CI lint is installed in the github actions workflows of the project. This lint
blocks pull requests from being merged if one or more of the commits of the pull
request misses the `Signed-off-by` trailer.

As a result, it is not possible to merge pull requests that miss the trailer.

## Coding style, Documentation, Testing

Coding style is enforced via `rustfmt` in its default configuration.
Compliance with the coding style is enforced via CI.

Source code documentation as well as other documentation is tested via github
actions workflows as well, to ensure that a developer is able to build all
relevant documentation on their local machine.

Testing is done via workflows in github actions using `cargo test --all-features` for all
crates.
The main objective with this is that a developer should be able to simply run
`cargo test` on their local machine to be able to see whether CI would succeed
for changes they submit in a pull request.
Thus, all CI tests (unit tests, but also integration tests) are implemented in
Rust and do not rely on external services.
End-to-end system tests - that depend on external services - are run outside the CI pipeline,
to avoid inconsistent test outcomes because of external issues.

## Reviewing, addressing feedback

Generally, pull requests need at least an approval from one maintainer to be
merged.

When addressing review feedback, it is helpful to the reviewer if additional
changes are made in new commits. This allows the reviewer to easily see the
delta between what they previously reviewed and the changes you added to address
their feedback. If applicable, the `git commit --fixup=<sha>` feature should be
used. These fixup commits should then be squashed (usually `git rebase -i
--autosquash main`) by the author of the PR after it passed review and before it
is merged.

## Pull request merging and evergreen-master

The project pursues the "evergreen master" strategy. That means that at every
point in time, the commit that `master`/`main` points to must successfully
build and pass all tests.

Because of that, merging is implemented in a certain way that forbids several
bad practices which could lead to a breaking build on the `master`/`main`
branch, and a special workflow is implemented to ensure not only the "evergreen
master" but also to streamline the workflow for integrating pull requests.

The following actions are **not** allowed:

* Committing to `master`/`main` directly.
  The GitHub repository is configured to block pushing to `master`/`main`.
* Squash-Merging of pull requests (which is equivalent to committing to
  `master`/`main` directly).
  The github repository is configured to not allow squash merges.
* Merging of "fixup!" or "squash!" commits. A github actions
  job is employed to block pull requests from being merged if such commits are
  in the branch of the pull request. Rebase should be used to clean those up.

It is explicitly _not_ forbidden to have merge-commits in a pull request.
Long-running pull requests that contain a large number of changes and are
developed over a long period might contain merges. They might even merge
`master`/`main` to get up-to-date with the latest developments. Though it is
not encouraged to have such long-running pull requests and discouraged to
merge `master`/`main` into a PR branch, the project acknowledges that
sometimes it is not avoidable.

To summarize, the following requirements have to be met before a pull request
can be merged:

* Reviews of the relevant people (ensured via github setting)
* Status checks (CI, lints, etc) (ensured via github setting)
  * builds, tests, lints are green (ensured via github action)
  * Commit linting (ensured via github action)
  * No missing `Signed-off-by` lines (ensured via github action)
  * No "fixup!"/"squash!" commits in the pull request (ensured via github
    action)

Merging itself is implemented via a "merge bot": [bors-ng](https://bors.tech).
bors-ng is used to prevent "merge skew" or "semantic merge conflicts"
(read more [here](https://bors.tech/essay/2017/02/02/pitch/)).

## Dependency updates

Dependencies should be kept in sync over all crates in the project. That means
that different crates of the project should try to use dependencies in the
same versions, but also that dependencies should be harmonized in a way that a
specific problem should not be solved with more than one external library at a
time.
Updates of dependencies is automated via a github bot
([dependabot](https://github.com/dependabot)).
To ensure harmonization of dependencies, a dedicated team (see "Team
Structure") is responsible for keeping an eye on the list of dependencies.

## License linting

License linting describes the act of checking the licenses of dependencies and
whether they meet a certain criteria.
For example, it is not feasible to import an external library that is licensed
as GPL-3.0 in an Apache-2.0 licensed codebase.
Because of this, a github action is installed to lint the licenses of
dependencies. This action runs as a normal lint (see "evergreen master") and
blocks pull requests if dependencies get imported that do not meet a set of
rules agreed upon by the project maintainers.

# Contributor License Agreement

We do not want to bother you with too much legalese, but there are two pages you
have to read carefully, this page and the CONTRIBUTOR LICENSE AGREEMENT.

## Signing the CONTRIBUTOR LICENSE AGREEMENT

Each Contribution to Software AG's Open Source Projects must be accompanied by a
sign-off indicating acceptance of current version of the CONTRIBUTOR LICENSE
AGREEMENT, which is derived from the Apache Foundation's Individual Contributor
License Agreement, sign-off time stamp relates to corresponding version of the
CONTRIBUTOR LICENSE AGREEMENT maintained here on GitHub as well. Sign-Off and
acceptance of the CONTRIBUTOR LICENSE AGREEMENT is declared by using  the option
"-s" in

> git commit -s

which will automatically generate a sign-off statement in the form:

> Signed-off-by: Max Mustermann \<MaxM@example.com\>

By adding this sign-off statement, you are certifying:

*By signing-off on this Submission, I agree to be bound by the terms of the
**then current CONTRIBUTOR LICENSE AGREEMENT** located at
https://github.com/thin-edge/thin-edge.io/blob/main/CONTRIBUTOR-LICENSE-AGREEMENT.md,
**which I have read and understood** and I agree that this Submission
constitutes a "Contribution" under this Agreement.*

## Note on Privacy

Please note that this project and any contributions to it are public and that a
record of all contributions (including any personal information submitted with
it, including a sign-off) is maintained indefinitely and may be redistributed
consistent with this project or the open source license(s) involved.

In addition to [GitHub's privacy
statement](https://docs.github.com/en/github/site-policy/github-privacy-statement)
extracting personal data from these projects for any other use than maintaining
the projects and communication related to it is prohibited, explicitly
prohibited is extracting email addresses for unsolicited bulk mails.

If you'd like to keep your personal email address private, you can use a
GitHub-provided no-reply email address as your commit email address. You can
choose which verified email address to author changes with when you edit,
delete, or create files or merge a pull request on GitHub. If you enabled email
address privacy, then the commit author email address cannot be changed and is
\<username\>@users.noreply.github.com by default.

See [setting your commit email address on GitHub](https://docs.github.com/en/github/setting-up-and-managing-your-github-user-account/setting-your-commit-email-address#setting-your-commit-email-address-on-github).

In the upper-right corner of any page, click your profile photo, then click
**Settings**.

1. In the left sidebar, click **Emails**.
1. In "Add email address", type your email address and click **Add**.
1. Verify your email address.
1. In the "Primary email address" list, select the email address you'd like to
   associate with your web-based Git operations.
1. To keep your email address private when performing web-based Git operations,
   click **Keep my email address private**.
1. You can use the git config command to change the email address you associate
   with your Git commits.

See [setting your commit email address in Git](https://docs.github.com/en/github/setting-up-and-managing-your-github-user-account/setting-your-commit-email-address#setting-your-commit-email-address-in-git).

1. Open Git Bash.
2. Set an email address in Git. You can use your GitHub-provided no-reply email
   addressor any email address. >$ git config --global user.email
   "email@example.com"
3. Confirm that you have set the email address correctly in Git:
>$ git config --global user.email <br>
>email@example.com
4. Add the email address to your account on GitHub, so that your commits are
   attributed to you and appear in your contributions graph.

