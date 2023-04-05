# Governance thin-edge.io

This documents serves as a guiding document to ease technical decision-making in the project as a whole.

Some of its goals are:

- Making sure that the different stakeholders are represented with their requirements
- Making sure that contributors know how decisions are made and who they can address concerns/questions to
- Making sure that team members know what is required of them for the project


## The project vision

In the [vision.md](./vision.md) the goal of the project is encoded.
It specifies the different functional requirements and non-functional requirements of the project as a whole.
All the stakeholders must agree on the vision upon joining.

- The current stakeholders are: Software AG, and IFM

Changes to the vision need to be done in a plenum of stakeholders, this makes sure that everyone is aware and agrees to the evolution of the project.


## Maintainership

In this project repository, some contributors have special rights to decide the
way forward of the project: the maintainers.

These special rights are:

- Merging new code
- Deciding on additions/changes

Translated into actions, this would be things like approving and merging PRs,
reviewing issues as well as discussions.

To become a maintainer, the current group of maintainers would simply choose to
promote them. No clear process is currently defined, but in general should a
motivated and trustworthy contributor be given precedence.

Maintainers should strive for agreement based on the following factors:

- The vision of the whole project
- Feasibility, accounting for future technical and cultural shifts
- The health of the project going forward
- The community being active and motivated

The maintainers ensure that all changes in the project align with the agreed
vision and support the health of the project and its community without being
unduly influenced from their employers.

## Team Structure

Underneath the maintainers are the teams or individuals, that each are allowed
to  make similar decisions about the project *in the area they have been
delegated*. Team members should be chosen depending on their merit and
trustworthiness.

Teams are composed of either a single or multiple contributors. 
Each team is assigned a single or multiple subtrees of the project, as defined
by the project structure. 
The extent of which depends on the area and expected workload.

They are responsible for only that specific part of the project, and as
such are tasked with maintaining it.
This means they should do the same tasks as the maintainers: Periodically
reviewing pull requests, triaging issues for their area, and general
maintenance of their codebase.
The teams follow the project hierarchy for decisions, i.e. higher decisions take precedence.

Decisions impacting multiple teams or the whole project need to be handled with special care.
They need to be discussed with all relevant teams and a consensus needs to be reached.
Some decisions are local, e.g. a component maintained by a company, but even in
this case the team still needs to be a 'good citizen' in the project and follow
agreed upon conventions (CI, format, etc..)


## Project Structure

The project has a hierarchy to solve these two problems:

1. How can users make sure that their voice and interests are represented in the project?
    - Each member party appoints one core team member
    - They form the group of people that have to, and are responsible for, signing off on the technical implementation of the project
2. How to make the lives of these team members easier?


-----

* Thin-edge is organized around sub-projects - to ease decision taking.
    * The sub-projects are organized in a hierarchy
        * This makes it easier to decouple different parts of the project
        * This makes it easier to decide on technical questions
    * Everyone has to agree on the core project as it is the foundations to all others.
    * At the periphery, sub-projects might be related to specific eco-systems (e.g. Cumulocity or Moneo) and therefore have independent decision processes.
    *	At an intermediate level, one might have sub-projects related to specific use-cases like "JSON over MQTT".
* All the sub-projects share a common repository - to ease consistency across projects and over time.
    *	Labels are used to organize issues/PRs/discussions along sub-projects.
    *	Code ownership is used to enforce cooperation around key components, but the default is to let things open and to trust each other, using version control as a safety net.






```
core contributors = [SAG, IFM]

-> Project leads, one each from [core contributors]
|
 \- Responsible for `/*`, aka everything in the repository
 |
 |
 \
  \-> Delegated project Teams, any combination of [teams, individual contributors]
  |
  |\- Responsible for `/plugins/plugin_{foo,bar}`
  ||- Are trusted by the project leads (who would have to stand-in for the trusted members)
  | 
  |->  Can be needed to nest further if the project grows bigger
```

## Release workflow

The following describes how thin-edge.io is released.

The following chapters do not talk about major releases (increasing "x" in a
"x.y.z" version number) as there is no workflow for these implemented yet.


### Preface

The following chapter describes the release process of the thin-edge.io project
and the corresponding crates.

The goal of the workflow described in this chapter is to have _as little impact
as possible_ on the general development of the project.
Releases should _never_ stop, prevent or even just slow down the normal
development of the project.
For example, halting development on the `main` branch and forbidding
contributors and maintainers to merge changes to it even for as little as one
hour just so that a new release can be published is not acceptable.
Also should "patch" releases (sometimes called "bugfix releases") have no impact
on the project pace or even concern developers - they should just happen in
parallel without any influence on the normal development activities.

With these requirements, we implement the following workflow.

### Semver conformity and API coverage

We adhere to [semver](https://semver.org),
with the [exceptions implied by the cargo implementation of the standard](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#specifying-dependencies-from-cratesio).

Parts of the API that are covered by semver are explicitly marked. Our public
API is extensively tested in integration tests. If changes to the public API of
thin-edge.io are necessary, these changes are documented in the `CHANGELOG.md`
file of the project, with reasoning why the change was necessary and how users
can upgrade to the new interface(s).

<!-- TODO: Expand on how exactly these changes are documented? -->

We also define the following rules for our release process:

* We release new features by increasing the "minor" version
* When we release new patch releases, these releases never contain new features,
  but only fixes to bugs.

### Release cycle

Minor releases are published roughly every TBD months.

Until a minor release is published, the previous minor release _can_ receive
patch releases (accordingly to [semver](https://semver.org/)).

### Strategy

We use the release-branch strategy, where development happens on the `main`
branch and releases are done on dedicated `release-X.Y.Z` branches.

Because we employ the "evergreen main branch" strategy and all of our merge
commits on the `main` branch are guaranteed to successfully build and pass CI, a
release can potentially be made starting from every commit on the `main` branch.

For a description of the release workflow, read below.

### Release maintainer

For every release, one or more "release maintainers" are appointed by the core team.

The responsibilities of the release maintainers are:

* Creation and maintenance of the `release-x.y.z` branches
* Cherry-picking bugfix patches from the `main` branch and applying them to the
  relevant `release-x.y.z` branch (via pull request to that branch)
  * If a patch does not apply cleanly, it is _not_ the responsibility of the
    release maintainer to make the patch apply. It is responsibility of the
    release maintainer, though, to talk to the developer of the patch and work
    with them to port the fix back to the release
* Create git-tags for the release(s) as appropriate
* Create github artifacts (or take care of a github-action pipeline that does)
  for releases
* Port the changes to the `CHANGELOG.md` file back to `main` (via pull request)
* Submit pull requests to the `main` branch to update all version numbers for
  the next release cycle

It is explicitly allowed for everyone to open bugfix-backport pull requests to
a release branch, but the release maintainer(s) decide when and how to apply
them to the release branch.

### Release branches

Release branches MUST be named with a pattern. This pattern is

```
"release" dash <major version number> dot <minor version number> dot "x"
```

The `"x"` is used instead of the "patch version number", because patch releases
are published from the release branches (read below).

A release branch gets the following rules enforced via GitHub:

* Only the "release-maintainers" team can create branches named `"release-*"`
* Pull requests are required on these branches
* Status checks are required to succeed on these branches
* "CODEOWNERS" is **not** enforced on these branches
* These rules are also enforced for administrators

### Workflow for minor releases

The following steps happen when a minor release is made:

1. One release maintainer creates a branch, starting from a recent commit on
   `main`, named after the aforementioned naming rule.
2. It is ensured that this branch cannot be pushed to by anyone except the
   [bors merge bot](https://bors.tech)
   (Refer to the "Merge strategies" point in the "Repository maintenance"
   chapter for more information)
3. The release maintainer crafts the `CHANGELOG.md` entries for this release and
   opens a pull request for it, targeting the aforementioned release branch.
4. Once agreed upon with everyone involved, they merge the PR for the
   `CHANGELOG.md` to the release branch
5. The release maintainer publishes the release on crates.io and creates a new
   tag for the release.
   They might add a tag message as they seem appropriate.
   They then push the tag to the GitHub repository of the project.
   They also make sure that release artifacts are published on GitHub for the
   new release (or make sure that a github-actions pipeline does this)
6. After the release is done, the `CHANGELOG.md` changes are cherry-picked to a
   pull request to the `main` branch by the release maintainer
7. After the release is done, the release maintainer opens a pull request to the
   `main` branch to update all version numbers of all crates to the next minor
   version

### Workflow for patch releases

Patch releases are published when appropriate, there is no fixed release cycle.
It might be a good rule-of-thumb to not release more than one patch release per
week.

#### Backporting

Bugfixes that are added to a patch release _must_ have been applied to the
`main` branch, before they are backported to the appropriate `release-x.y.z`
branch.
If a bugfix that is applied to `main` cannot be backported, due to either
conflicts or because the feature that is about to be fixed does not exist in the
`release-x.y.z` branch anymore, the patch may be adapted to apply cleanly or a
special patch may be crafted.
The release maintainer is encouraged to reach out to the developer of the patch,
as it is _not_ the responsibility of the release maintainer to adapt patches.

In any case, fixes that are backported to a `release-x.y.z` branch, MUST pass
CI and thus MUST be submitted to the release branch via a pull request.

#### Publishing of patch releases

A patch release is published as soon as the release maintainer thinks it is
appropriate.

The steps the release maintainer follows are almost the same as the steps of the
minor release workflow:

* The release maintainer submits a pull request to the release branch with the
  following changes:
  * `CHANEGLOG.md` entries for the patch level release
  * Updates of the version numbers for all crates in the "patch"-level position,
    e.g. "0.42.0" becomes "0.42.1",
* Once the aforementioned pull request is merged, the release maintainer
  publishes the release on crates.io, tags (`git tag -a vMAJOR.MINOR.PATCH`)
  the release and pushes the tag to the GitHub repository of the project
* Once the release is published, the release maintainer opens a pull request to
  the `main` branch of the project, porting the changes that were made to the
  `CHANGELOG.md` file to this branch

### More

The `CHANGELOG.md` is added on the release branch rather than on the `main`
branch. The reason for this is the workflow: If the changelog is adapted on
`main`, the branchoff for the release branch can _only_ happen directly after
the changelog was merged, because other merges may introduce new changes that
need to be in the changelog. Also, the changelog-adding pull request must be up
to date all the time, if patches get merged to `main` _while_ the changelog is
written, the changelog would need further fixes.
If the changelog is added on the release branch, no synchronization is
necessary, as the branchoff for the release already happened.
Instead, the changelog entries can be "backported" to `main`, which is trivial.

In a similar fashion are patch-level `CHANGELOG.md` entries ported to the `main`
branch.

Version number bumps happen right _after_ branchoff for a release. Doing the
version number bump before the release would mean that the release maintainers
would have to wait for the version-bump-pull-request, which is not acceptable
under the preassumption that every commit from `main` can potentially be
released.
By bumping the numbers right _after_ the release, but for the next release, we
automatically get a peace-of-mind state for that next release, where the release
maintainers can again just take any commit on `main` and start their release
process.

The implication of the patch-level release workflow is that the `main` branch
does never see changes to the version strings of the crates in the "patch" level
position. This is intentional, as there is no need for that.


## Related

* [Understanding open source governance models](https://www.redhat.com/en/blog/understanding-open-source-governance-models)
* [Producing Open Source Software](https://producingoss.com/en/producingoss.html)
