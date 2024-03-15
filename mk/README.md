# Notes

The scripts in this folder are taken from the [briansmith/ring](https://github.com/briansmith/ring) repository under the folder. This is done as the build targets of thin-edge.io are coupled with the build targets supported by ring (as it is the crypto library used in the thin-edge.io usage of rustls and some other libraries).


## How to update the mk files from ring

1. Navigate to the [mk folder in the briansmith/ring](https://github.com/briansmith/ring/tree/main/mk) repository

2. Copy the following files into the `mk` folder of thin-edge.io

    **Files that don't require patching**

    * mk/llvm-snapshot.gpg.key

    **Files that require minimal patching**

    THe following files require minor patching to add some additional thin-edge.io specific changes, however adjusting should be fairly easy to manage using the git diff:

    * mk/cargo.sh
    * mk/install-build-tools.sh

    Some of the changes are due to shell check warnings, and there is already an [upstream PR](https://github.com/briansmith/ring/pull/1993) exists to resolved this warnings) so that this step can be skipped once the PR is merged. Alternatively you can take the files from the aforementioned PR instead of the PR itself if you are unsure how to merge the shell check changes.

    **Notes**
    * The copyright notice at the top of each files originating from **briansmith/ring** *MUST* be preserved

3. Review the changes and resolve any differences

4. Create a PR with the updated changes
