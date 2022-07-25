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



## Team Structure

* Teams are either single contributors or a group of them
* They are responsible for a specific part of project, and as such are the maintainers of those parts
    * This includes: Reviewing pull requests, triaging issues in their assigned area, and general maintenance
* Each team is assigned a single or multiple subtrees of the project, as defined by the project structure
* The teams follow the project hierarchy for decisions, i.e. higher decisions take precedence

The important bits:

At the 'top' are the maintainers, whose job is to define and realize the project vision of thin-edge.io.
Maintainers should strive for agreement based on the following factors:

- The vision of the whole project
- Feasibility, accounting for future technical and cultural shifts
- The health of the project going forward

Overall, the maintainers should not be beholden to their respective companies, and instead to the health of the project and its community.

Underneath the leads are the teams or individuals, that each are allowed to  make similar decisions about the project *in the area they have been delegated*.


- Some decisions are global in the project, like the core/API
    - Team members should try to make sure to explore possible options together before calling for time-consuming solutions like a vote. This mostly includes uncontroversial changes that are low impact or have already been agreed upon
    - If an exclusive choice has to be made (where it is not possible to entertain two conflicting approaches), and no clear side is more advantageous to pick, a vote should be held with the majority opinion being the final decision.

- Some decisions are local, e.g. a plugin does not impact others with its behaviour, however it still needs to be a 'good citizen' in the project (CI, format, etc..)
    - These should be clearly scoped in their respective project part
    - However, if needed a 'higher up' decision can be requested if no consensus is achieved



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
    *	At an intermediate level, one might have sub-projects related to specific use-cases like “JSON over MQTT”.
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

## Repository maintenance

To assure a consistent level of quality and appearance, the project has some rules for maintainers to follow:

- **Clear commits**
  Having clear, concise and well-written commit messages for all changes is not
  only a indicator for the seriousness of the project, but also a way to ensure
  sustainable development via repeatability of the sources of the project.

  Because of this, we try to ensure the highest possible quality for each
  individual commit message.

  Because such a thing cannot necessarily or in full be enforced via tooling, it
  remains mainly the obligation of a reviewer of a change-set to ensure

  * Exclusiveness of the individual commits
  * that one commit is one atomic change, and not an accumulation of several
    individual and separate changes
  * that the commit message of the change expresses why the change was made

  Every reviewer of a pull request is asked to review not only the changes, but
  also the commit messages.
  Reviewers are explicitly empowered and actually encouraged to block pull
  requests that contain commit messages that do not meet a certain standard
  even and especially also if the proposed changes are acknowledged.

  The contributor should document why they implemented a change properly. A good
  rule of thumb is that if a change is larger than 60 lines, a commit message
  without a body won't suffice anymore.
  The more lines are touched by an individual commit, the more lines should be
  used to explain the change.

  The project implements lints using github actions to meet such criteria.

  Also, some hard criteria is checked via github action lints:

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

  Commits using the `--fixup` or `--squash` feature of `git-commit` are allowed,
  but won't be merged (read more about this in the "merge strategies" section).

- **Ensuring the Signed-off-by-trailer**
  Contributors need to sign the "Contributor License Agreement". They do so by
  signing off each individual commit with a so-called "Signed-off-by Trailer".
  Git offers the `--signoff`/`-s` flags when using `git-commit` to commit
  changes.

  To ensure that all commits have the trailer present in the commit message, a
  CI lint is installed in the github actions workflows of the project. This lint
  blocks pull requests to be merged if one or more of the commits of the pull
  request misses the `Signed-off-by` trailer.

  As a result, it is not possible to merge pull requests that miss the trailer.
- **Coding styleguide**
  Coding style is enforced via `rustfmt` in its default configuration.
  Compliance with the coding style is enforced via CI.
- **Testing**
  Testing is done via workflows in github actions using `cargo test --all-features` for all
  crates.
  The main objective with this is that a developer should be able to simply run
  `cargo test` on their local machine to be able to see whether CI would succeed
  for changes they submit in a pull request.
  Thus, all CI tests (unit tests, but also integration tests) are implemented in
  Rust and do not rely on external services.
  End-to-end system tests - that depend on external services - are run outside the CI pipeline,
  to avoid inconsistent test outcomes because of external issues.
- **Benchmarks**
- **Documentation builds**
  Source code documentation as well as other documentation is tested via github
  actions workflows as well, to ensure that a developer is able to build all
  relevant documentation on their local machine.
- **Keeping spec up to date**
- **Evergreen master**
  The project pursues the "evergreen master" strategy. That means that at every
  point in time, the commit that `master`/`main` points to must successfully
  build and pass all tests successfully.

  Reaching that goal is possible because of the employed merge strategies (read
  below).
- **Merge strategies**
  Merging is the way how the project accepts and implements changes.

  Because of the pursued "everygreen master", merging is implemented in a
  certain way that forbids several bad practices which could lead to a breaking
  build on the `master`/`main` branch, and a special workflow is implemented to
  ensure not only the "evergreen master" but also to streamline the workflow for
  integrating pull requests.

  The following actions are **not** allowed:

  * Committing to `master`/`main` directly
  * Squash-Merging of pull requests (which is equivalent to committing to
    `master`/`main` directly).
    The github repository is configured to not allow squash merges.
  * Merging of "!fixup" or "!squash" commits. A github actions
    job is employed to block pull requests from being merged if such commits are
    in the PR branch. Rebase should be used to clean those up.

  It is explicitly _not_ forbidden to have merge-commits in a pull request.
  Long-running pull requests that contain a large number of changes and are
  developed over a long period might contain merges. They might even merge
  `master`/`main` to get up-to-date with the latest developments. Though it is
  not encouraged to have such long-running pull requests and discouraged to
  merge `master`/`main` into a PR branch, the project acknowledges that
  sometimes it is not avoidable.

  To summarize, the following requirements have to be met before a pull request
  can be merged:

  * Reviews of the relevant persons (ensured via github setting)
  * Status checks (CI, lints, etc) (ensured via github setting)
      * builds, tests, lints are green (ensured via github action)
      * Commit linting (ensured via github action)
      * No missing `Signed-off-by` lines (ensured via github action)
      * No "!fixup"/"!squash" commits in the pull request (ensured via github
        action)

  Merging itself is implemented via a "merge bot": [bors-ng](https://bors.tech).
  bors-ng is used to prevent "merge skew" or "semantic merge conflicts"
  (read more [here](https://bors.tech/essay/2017/02/02/pitch/)).
- **Dependency updates**
  Dependencies should be kept in sync over all crates in the project. That means
  that different crates of the project should try to use dependencies in the
  same versions, but also that dependencies should be harmonized in a way that a
  specific problem should not be solved with more than one external library at a
  time.
  Updates of dependencies is automated via a github bot
  ([dependabot](https://github.com/dependabot)).
  To ensure harmonization of dependencies, a dedicated team (see "Team
  Structure") is responsible for keeping an eye on the list of dependencies.
- **License linting**
  License linting describes the act of checking the licenses of dependencies and
  whether they meet a certain criteria.
  For example, it is not feasible to import an external library that is licensed
  as GPL-3.0 in an Apache-2.0 licensed codebase.
  Because of this, a github action is installed to lint the licenses of
  dependencies. This action runs as a normal lint (see "evergreen master") and
  blocks pull requests if dependencies get imported that do not meet a set of
  rules agreed upon by the project.

## Release workflow

The following describes how thin-edge.io is released.

The following chapters do not talk about major releases (increasing "x" in a
"x.y.z" version number) as there is no workflow for these implemented yet.


### Preface

The following chapter describes the release process of the thin-edge.io project
and the corrosponding crates.

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
thin-egde.io are necessary, these changes are documented in the `CHANGELOG.md`
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

It is explicitely allowed for everyone to open bugfix-backport pull requests to
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
