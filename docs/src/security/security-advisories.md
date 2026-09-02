---
title: Security Advisories
tags: [Security]
sidebar_position: 2
description: How %%te%% publishes security advisories, emergency releases and safety guidance
---

Once a vulnerability has been fixed, or once a mitigation is available,
the %%te%% project informs its users through two complementary channels:
a GitHub security advisory, which is the authoritative and machine-readable record,
and an announcement in the Tech Community, which reaches users who do not follow the repository.

## GitHub security advisories

Advisories are published on the
[security advisories page](https://github.com/thin-edge/thin-edge.io/security/advisories)
of the %%te%% repository.

Each advisory states:

- a description of the vulnerability and of its impact,
- the affected versions and the version in which the problem is fixed,
- a severity rating,
- any workaround which can be applied when upgrading is not immediately possible,
- credit to the reporter, unless they asked to stay anonymous.

Where appropriate, a CVE identifier is requested through GitHub when the advisory is published.
Published advisories are added to the
[GitHub Advisory Database](https://github.com/advisories),
so that tools such as Dependabot and other vulnerability scanners
can detect an affected version of %%te%%.

## Announcements in the Tech Community

Security announcements are published in the
[Tech Community](https://techcommunity.cumulocity.com/)
under the [tedge-security](https://community.cumulocity.com/tag/tedge-security/1617) tag.

This tag is used for all the notifications which require the attention of an operator,
and not only for advisories:

- **Security advisories**, linking to the corresponding GitHub advisory and to the release containing the fix.
- **Emergency releases**, when a release is published outside of the regular release cycle.
- **Safety guidance**, such as a recommended configuration change, a workaround for an unfixed problem,
  or a warning about an insecure usage pattern.

To be notified, open the
[tedge-security](https://community.cumulocity.com/tag/tedge-security/1617) tag
while logged in to the Tech Community, and watch it.

:::note
A Tech Community announcement never replaces the advisory.
When the two differ, the GitHub advisory is the reference.
:::

## Emergency releases

A fix for a vulnerability is released as a patch release,
published through the same channels as any other release:
the [GitHub releases](https://github.com/thin-edge/thin-edge.io/releases) page,
the Cloudsmith package repositories and the container images.
See [Installation](../install/index.md) for the details of each channel.

An emergency release is a patch release which is published outside of the regular release cycle,
because the severity of the problem does not allow waiting for the next planned release.
Such a release contains the security fix and as few unrelated changes as possible,
so that it can be adopted quickly and with a low risk of regression.

Emergency releases are announced under the
[tedge-security](https://community.cumulocity.com/tag/tedge-security/1617) tag,
with a description of the problem, the affected versions and the upgrade path.

## Staying informed

You are recommended to use at least one of the following:

- Watch the [tedge-security](https://community.cumulocity.com/tag/tedge-security/1617) tag in the Tech Community.
- Watch the %%te%% repository on GitHub, selecting **Custom** &rarr; **Security alerts** and **Releases**,
  to be notified of new advisories and releases.
- Track the versions of %%te%% which are deployed on your devices,
  so that you can determine quickly whether an advisory applies to your fleet.
