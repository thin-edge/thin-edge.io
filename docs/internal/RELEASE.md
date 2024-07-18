# Release Flow

The following steps describes the thin-edge.io release process:

1. Trigger the [release workflow](https://github.com/thin-edge/thin-edge.io/actions/workflows/release.yml) in thin-edge.io repository. Select the appropriate version increment (e.g. **patch** for bug fix releases, or **minor** for non-breaking feature releases).

   **Notes**

   * The workflow will create a new [PR](https://github.com/thin-edge/thin-edge.io/pulls) in the repository.

2. Review the [PR](https://github.com/thin-edge/thin-edge.io/pulls) (created in the previous step) and approve

   **Notes**

   When this PR is merged, the [autotag](/.github/workflows/autotag.yml) workflow will detect the change in official version in the Cargo.toml, and then tag the commit with the new version number. This in turn triggers the [build-workflow](https://github.com/thin-edge/thin-edge.io/actions/workflows/build-workflow.yml) yet again to build the artifacts with the official version number.

   During a release, the [build-workflow](https://github.com/thin-edge/thin-edge.io/actions/workflows/build-workflow.yml) will do the following:

   * Create a draft Release
   * Publish the Linux packages to the `tedge-main*` channel in [Cloudsmith](https://cloudsmith.io/~thinedge/repos/)
   * Promote the Linux packages from `tedge-main*` to `tedge-release*` in [Cloudsmith](https://cloudsmith.io/~thinedge/repos/)
   * Build and publish container images to the [tedge](https://github.com/thin-edge/thin-edge.io/pkgs/container/tedge) package in the Github container registry
   * Create a PR in the [tedge-docs](https://github.com/thin-edge/tedge-docs/pulls) repository which includes a new snapshot of the docs for the new official version

3. Review the [tedge-docs snapshot PR](https://github.com/thin-edge/tedge-docs/pulls), and approve/merge it

   **Notes**

   When a new documentation snapshot is created, only the latest major version of each snapshot will be kept.

   When the PR is merged, it will trigger a new deployment of the [Github Pages](https://github.com/thin-edge/thin-edge.io/actions/workflows/gh-pages.yml) with the option enabled to update the Algolia search index (e.g. by triggering the Algolia crawler).

4. Review the [Github draft release](https://github.com/thin-edge/thin-edge.io/releases) and update the changelog accordingly

5. Publish the [Github release](https://github.com/thin-edge/thin-edge.io/releases) from the previous step.


The above process can be visualized by the following git process:

```mermaid
gitGraph
   commit
   commit
   branch feat
   checkout feat
   commit
   checkout main
   merge feat
   commit id: "Trigger Release"
   branch release
   checkout release
   commit id: "bump cargo version"
   checkout main
   merge release
   commit tag: "1.0.1"
```


## Changelog generation

Changelog generation is provided by [git-cliff](https://github.com/orhun/git-cliff) combining both commit history and Github PR title and labels.

The following details which information is used to build the changelog:

* The PR title is used as the changelog entry. This allows the title to be modified without having to amend commits.

* Github labels which start with "theme:" are used to set the scope of the PR (e.g. which component the PR is related to), e.g. `theme:software`, `theme:mqtt` etc.

* A PR can be excluded from the changelog by adding the label "skip-release-notes"


### PR Checklist

The following items should be included in PRs to ensure they are ready for changelog generation:

* PR titles are human readable and follow the format:

   ```
   <type>: <description>
   ```

   See [Types](./RELEASE.md#types) for a list of recommended values.

* At least one "theme:*" label should be added to the PR to indicate which components are affected by the PR

#### Types

One of the following types (prefixes) MUST be used:

* feat:
* fix:
* build:
* chore:
* ci:
* docs:
* style:
* refactor:
* perf:
* test:
