# Release Flow

The following steps describes the thin-edge.io release process:

1. Trigger Workflow in thin-edge.io repo
2. Review PR and approve 

   **Notes**

   When this PR is merged, it the [autotag](.github/workflows/autotag.yml) workflow will detect the change in official version in the Cargo.toml, and then tag the commit with the new version number. This in turn triggers the [build-workflow](build-workflow) yet again to build the artifacts with the official version number.

   * A draft Release will be created
   * Linux packages will be published to cloudsmith to the `tedge-main*` channel
   * Linux packages will be promoted from `tedge-main*` to `tedge-release*`
   * Container image will be built and published to `tedge-release`
   * A new PR will be created in the [tedge-docs](https://github.com/thin-edge/tedge-docs/pulls) project to create a snapshot related to the new release

3. Review the [tedge-docs snapshot PR](https://github.com/thin-edge/tedge-docs/pulls), and merge (this will trigger the docs to be updated)
   **Notes**

   You may need to edit the PR if necessary if you also want to remove an existing version in the same PR.

4. Review [Github draft release](https://github.com/thin-edge/thin-edge.io/releases) and update the changelog
5. Publish the [Github release](https://github.com/thin-edge/thin-edge.io/releases) from step 3.
6. Trigger the Algolia crawler to update the search index (once the new version of the website is published and available)


```mermaid
gitGraph
   commit
   commit
   branch feat
   checkout feat
   commit
   checkout main
   merge feat
   commit id: "Trigger release"
   branch release
   checkout release
   commit id: "bump cargo version"
   checkout main
   merge release
   commit tag: "1.0.1"
```

## Goals

* Generate more descriptive changelogs automatically
   * Feature/fix etc.
   * Add categories to show which components were affected, e.g. firmware, software, config, troubleshooting
* Validate that a PR conforms

## Using Github features

* Use labels to control communication of breaking changes (allows post commit classification what a "breaking change" is)


## Other solutions - github based

https://github.com/release-drafter/release-drafter?tab=readme-ov-file

## Conventional commits

[Conventional commits](https://www.conventionalcommits.org/en/v1.0.0/#summary) are used to formally communicate intent of code in a structure manner to allow other processes to parse the commit messages. This is commonly used to help generate change logs from commit messages.

### Types

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

### Scopes

Scope is optional, but is used to indicate which topic the feature is referring too.

Scopes can also be supplied by a github label.


### How to highlight notable changes?

* Use label "highlight" to add notable changes

## git-cliff

* search for the conventional commits, use github info

## cocogitto

https://docs.cocogitto.io/guide/

* A little too inflexible as it enforces that all commits (except merge commits) need to adhere to the "conventional commits" standard

   * You can rewrite non-conventional commits, but it would change the git commit hash. See https://docs.cocogitto.io/guide/#rewrite-non-compliant-commits

