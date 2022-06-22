# Governance thin-edge.io

This documents servers as a guiding document to ease technical decision making in the project as a whole.

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
    * This includes: Reviewing pull requests, triaging issues assigned to them, and general maintenance
* Each team is assigned a single or multiple subtrees of the project, as defined by the project structure
* The teams follow the project hierarchy for decisions, i.e. higher decisions take precedence

The important bits:

At the top, each invested party has their representative, who together with the others in their group have to come to conclusions together. These should be based on the following factors:

- The signed off vision of the whole project
- Feasability, accounting for future technical and cultural shifts
- The health of the project going forward

Overall, these 'project leads' should not be beholden to their respective companies, and instead to the health of the project and its community.

Underneath the leads are the teams or individuals, that each are allowed to  make similar decisions about the project *in the area they have been delegated*.


- Some decisions are global in the project, like the core/API
    - Team members should try and make sure to explore possible options together before calling for a vote. This mostly includes uncontroversial changes that are low impact or have already been agreed upon
    - If this is not possible, a vote needs to be held: TODO

- Some decisions are local, e.g. a plugin does not impact others with its behaviour, however it still needs to be a 'good citizen' in the project (CI, format, etc..)
    - These should be clearly scoped in their respective project part
    - However, if needed a 'higher up' decision can be requested if no consensus is achieved



## Project Structure

The project has a hierarchy to solve these two problems:

1. How can participating organizations make sure that their voice and interests are represented in the project?
    - Each member party appoints one core team member
    - They form the group of people that have to, and are responsible for, signing off on the technical implementation of the project
2. How to make the lives of these team members easier?


-----

* Thin-edge is organized around sub-projects - to ease decision taking.
    * The sub-projects are organized in a hierarchy
        * This makes it easier to decouple different parts of the project
        * This makes it easier to decide on technical questions
    * Everyone has to agree on the core project bringing the foundations to all others.
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


- clear commits
- Ensuring the Signed-off-by-trailer
  Contributors need to sign the "Contributor License Agreement". They do so by
  signing off each individual commit with a so-called "Signed-off-by Trailer".
  Git offers the `--signoff`/`-s` flags when using `git-commit` to commit
  changes.

  To ensure that all commits have the trailer present in the commit message, a
  CI lint is installed in the github actions workflows of the project. This lint
  blocks pull requests to be merged if one or more of the commits of the pull
  request misses the `Signed-off-by` trailer.

  As a result, it is not possible to merge pull requests that miss the trailer.
- coding styleguide
- testing
- benchmarks
- documentation builds
- keeping spec up to date
- evergreen master
  The project pursues the "evergreen master" strategy. That means that at every
  point in time, the commit that `master`/`main` points to must successfully
  build and pass all tests successfully.

  Reaching that goal is possible because of the employed merge strategies (read
  below).
- merge strategies
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

  It is explicitely _not_ forbidden to have merge-commits in a pull request.
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
  * Conversations must be resolved (ensured via github setting)

  Merging itself is implemented via a "merge bot": [bors-ng](https://bors.tech).
  bors-ng is used to prevent "merge skew" or "semantic merge conflicts"
  (read more [here](https://bors.tech/essay/2017/02/02/pitch/)).
- Dependency updates
- License linting

## Related

* [Understanding open source governance models](https://www.redhat.com/en/blog/understanding-open-source-governance-models)
* [Producing Open Source Software](https://producingoss.com/en/producingoss.html)
