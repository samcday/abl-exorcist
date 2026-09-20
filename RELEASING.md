# Releasing the assembler

[release-plz](https://release-plz.dev/docs/github/quickstart) manages versions,
changelogs, crates.io publication, and GitHub releases for
`abl-exorcist-assembler`. The freestanding `abl-exorcist` shim is excluded;
this workflow does not build or distribute device-specific shim binaries.

Each push to `main` updates a release PR when there are package changes.
Merging that PR triggers publication and a `v<version>` GitHub release.
`release_always = false` prevents ordinary commits from publishing a crate.
Review the version, changelog, and CI results before merging a release PR.

## Repository setup

In Settings → Actions → General, enable **Allow GitHub Actions to create and
approve pull requests**. The workflow uses the built-in `GITHUB_TOKEN`.
Because [that token does not trigger pull-request workflows](https://release-plz.dev/docs/github/token),
manually close and reopen a release PR to run CI on its current commit.
Repeat this after release-plz updates the PR, and wait for CI before merging.

## First publication: 0.0.1

crates.io currently requires the first publication of a new crate to use a
regular API token; [trusted publishing](https://crates.io/docs/trusted-publishing)
can then handle subsequent versions.

1. Merge this setup and let release-plz prepare the initial release PR.
   Confirm that the assembler stays at `0.0.1`, then run CI as described above.
2. Check out the reviewed release PR commit in a clean checkout, authenticate
   locally with crates.io, and publish:

   ```sh
   cargo publish -p abl-exorcist-assembler --locked --dry-run
   cargo publish -p abl-exorcist-assembler --locked
   ```

3. Configure a crates.io GitHub trusted publisher for `abl-exorcist-assembler`:

   | Field | Value |
   | --- | --- |
   | Repository owner | `samcday` |
   | Repository name | `abl-exorcist` |
   | Workflow filename | `release-plz.yml` |
   | Environment | Leave unset |

4. Merge that same release PR without further package changes, then check out
   its merged commit. Release-plz skips versions already on crates.io, so create
   the initial tag and GitHub release manually from that commit:

   ```sh
   gh release create v0.0.1 --target "$(git rev-parse HEAD)" --title v0.0.1 \
     --notes-file abl-exorcist-assembler/CHANGELOG.md
   ```

The publish job grants `id-token: write`; release-plz exchanges the GitHub OIDC
identity for a short-lived crates.io token itself. No `CARGO_REGISTRY_TOKEN`
repository secret or separate authentication action is needed.

## Subsequent releases

Review the release PR. If a version bump makes the standalone consumer lockfile
stale, run `cargo update --manifest-path tests/no-std-consumer/Cargo.toml --workspace`
on the release branch and commit the lockfile update. Trigger and check CI, then
merge the PR. The publish job
publishes the crate and creates its tag and GitHub release. If publication or
GitHub release creation fails before publication, fix the cause and rerun that
failed workflow run at the same commit. If the crate was published but its tag
or GitHub release is missing, complete those manually at the release commit;
release-plz skips versions already published or tagged.
