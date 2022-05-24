
# Contributing to thin-edge.io

Thanks for taking the time to contribute to thin-edge.io!

Contributing is not limited to writing code and submitting a PR. Feel free to submit an [issue](https://github.com/thin-edge/thin-edge.io/issues) or comment on an existing one to report a bug, provide feedback, or suggest a new feature. You can also join us on GitHub Discussions.

Of course, contributing code is more than welcome! If you're planning to submit a PR to implement a new feature or fix a bug, please open an issue that explains the change and the motivation for it.

If you are interested in contributing documentation, please note the following:

- Doc issues are labeled with the `doc` label.
- The thin-edge.io docs content is in the `docs/src/` directory.

[How to build from source.](./docs/src/BUILDING.md)

<br/>
<br/>

# Pull request and git commit guidance

## Opening PRs and organizing commits

PRs should generally address only 1 issue at a time. If you need to fix two bugs, open two separate PRs. This will keep the scope of your pull requests smaller and allow them to be reviewed and merged more quickly.

When possible, fill out as much detail in the pull request template as is reasonable. Most important is to reference the GitHub issue that you are addressing with the PR.

**NOTE:** GitHub has [a feature](https://docs.github.com/en/github/managing-your-work-on-github/linking-a-pull-request-to-an-issue#linking-a-pull-request-to-an-issue-using-a-keyword) that will automatically close issues referenced with a keyword (such as "Fixes") by a PR or commit once the PR/commit is merged. Don't use these keywords. We don't want issues to be automatically closed. We want our testers to independently verify and close them.

## Writing good commit messages

Git commit messages should explain the how and why of your change and be separated into a brief subject line followed by a more detailed body. When in doubt, follow this guide for good commit messages and you can’t go wrong: [https://chris.beams.io/posts/git-commit/](https://chris.beams.io/posts/git-commit/).

## Reviewing, addressing feedback, and merging

Generally, pull requests need at least an approval from one maintainer to be merged.

When addressing review feedback, it is helpful to the reviewer if additional changes are made in new commits. This allows the reviewer to easily see the delta between what they previously reviewed and the changes you added to address their feedback.
If applicable, the `git commit --fixup=<sha>` feature should be used.
These fixup commits should then be squashed
(usually `git rebase -i --autosquash main`) by the author of the PR after it
passed review and before it is merged.

Once a PR has the necessary approvals, it can be merged. Here’s how the merge should be handled:

- If the PR is a single logical commit, the merger should use the “Rebase and merge” option. This keeps the git commit history very clean and simple and eliminates noise from "merge commits."
- If the PR is more than one logical commit, the merger should use the “Create a merge commit” option.
- If the PR consists of more than one commit because the author added commits to address feedback, the commits should be squashed into a single commit (or more than one logical commit, if it is a big feature that needs more commits). This can be achieved by the “Squash and merge” option. If they do this, the merger is responsible for cleaning up the commit message according to the previously stated commit message guidance.
<br/>
<br/>

# Contributor License Agreement

We do not want to bother you with too much legalese, but there are two pages you have to read carefully, this page and the CONTRIBUTOR LICENSE AGREEMENT.

## Signing the CONTRIBUTOR LICENSE AGREEMENT

Each Contribution to Software AG's Open Source Projects must be accompanied by a sign-off indicating acceptance of current version of the CONTRIBUTOR LICENSE AGREEMENT, which is derived from the Apache Foundation's Individual Contributor License Agreement, sign-off time stamp relates to corresponding version of the CONTRIBUTOR LICENSE AGREEMENT maintained here on GitHub as well. Sign-Off and acceptance of the CONTRIBUTOR LICENSE AGREEMENT is declared by using  the option "-s" in

> git commit -s

which will automatically generate a sign-off statement in the form:

> Signed-off-by: Max Mustermann \<MaxM@example.com\>

By adding this sign-off statement, you are certifying:

*By signing-off on this Submission, I agree to be bound by the terms of the **then current CONTRIBUTOR LICENSE AGREEMENT** located at https://github.com/thin-edge/thin-edge.io/blob/main/CONTRIBUTOR-LICENSE-AGREEMENT.md, **which I have read and understood** and I agree that this Submission constitutes a "Contribution" under this Agreement.*

## Note on Privacy

Please note that this project and any contributions to it are public and that a record of all contributions (including any personal information submitted with it, including a sign-off) is maintained indefinitely and may be redistributed consistent with this project or the open source license(s) involved.

In addition to [GitHub's privacy statement](https://docs.github.com/en/github/site-policy/github-privacy-statement) extracting personal data from these projects for any other use than maintaining the projects and communication related to it is prohibited, explicitly prohibited is extracting email addresses for unsolicited bulk mails.

If you'd like to keep your personal email address private, you can use a GitHub-provided no-reply email address as your commit email address. You can choose which verified email address to author changes with when you edit, delete, or create files or merge a pull request on GitHub. If you enabled email address privacy, then the commit author email address cannot be changed and is \<username\>@users.noreply.github.com by default.

See [setting your commit email address on GitHub](https://docs.github.com/en/github/setting-up-and-managing-your-github-user-account/setting-your-commit-email-address#setting-your-commit-email-address-on-github).

In the upper-right corner of any page, click your profile photo, then click **Settings**.

1. In the left sidebar, click **Emails**.
1. In "Add email address", type your email address and click **Add**.
1. Verify your email address.
1. In the "Primary email address" list, select the email address you'd like to associate with your web-based Git operations.
1. To keep your email address private when performing web-based Git operations, click **Keep my email address private**.
1. You can use the git config command to change the email address you associate with your Git commits.

See [setting your commit email address in Git](https://docs.github.com/en/github/setting-up-and-managing-your-github-user-account/setting-your-commit-email-address#setting-your-commit-email-address-in-git).

1. Open Git Bash.
2. Set an email address in Git. You can use your GitHub-provided no-reply email addressor any email address.
>$ git config --global user.email "email@example.com"
3. Confirm that you have set the email address correctly in Git:
>$ git config --global user.email <br>
>email@example.com
4. Add the email address to your account on GitHub, so that your commits are attributed to you and appear in your contributions graph.

Release date: 2021-03-29
