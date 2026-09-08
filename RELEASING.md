# Releasing Vibedoc

Vibedoc releases the Rust CLI and official TypeScript adapter as separate
artifacts. The artifacts use the same version in v0.1.x, and the
`Schwick3/tap/vibedoc` formula installs both. The adapter is a checksummed
formula resource rather than a second formula because Homebrew does not
implicitly trust third-party formula dependencies.

## One-time Homebrew setup

1. Create the public `Schwick3/homebrew-tap` repository with `main` as its
   default branch.
2. Copy the contents of `packaging/homebrew/tap` into the new repository and
   copy the root `LICENSE` file.
3. Create a fine-grained GitHub token with read and write access to repository
   contents for `Schwick3/homebrew-tap` only.
4. Add the token to `Schwick3/vibedoc` as the Actions secret
   `HOMEBREW_TAP_TOKEN`.
5. Protect the secret from pull-request workflows. The release workflow only
   uses the secret for a pushed release tag after all formula tests pass.

## Prepare a release

1. Update the version in the root `Cargo.toml` and
   `adapters/typescript/package.json`.
2. Run `npm install --package-lock-only` when the adapter version changes so
   `package-lock.json` has the same version.
3. Confirm the intended tag matches every package:

   ```sh
   node scripts/release-version.mjs v0.1.1
   ```

4. Run the normal checks:

   ```sh
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   npm ci
   npm test
   ./scripts/test-e2e.sh
   ./scripts/test-evaluation.sh
   ```

5. Run the `Release` workflow manually with the proposed tag. A manual run
   builds and uploads workflow artifacts but does not create a GitHub release
   or modify the tap.

## Publish

Create and push an annotated stable-version tag:

```sh
git tag -a v0.1.1 -m "Vibedoc v0.1.1"
git push origin v0.1.1
```

The tag workflow:

1. validates the package versions;
2. runs the complete test suite on macOS and Linux;
3. builds four native CLI archives and one TypeScript adapter archive;
4. creates `SHA256SUMS` and the public GitHub release;
5. generates and tests the Homebrew formula on Intel and ARM macOS and Linux;
   and
6. commits the formula to `Schwick3/homebrew-tap`.

The release is installable when the tap commit succeeds:

```sh
brew install Schwick3/tap/vibedoc
vibedoc --version
vibedoc doctor
```

## Failure recovery

If artifact creation fails, fix the source and create a new patch version. Do
not move a published release tag.

If the GitHub release succeeds but formula testing or tap publication fails
because of a transient service or credential problem, correct that external
problem and rerun the failed jobs.

If the release assets are valid but the release workflow itself must change,
fix the workflow on `main` and run the `Publish Homebrew` workflow with the
existing immutable tag. This recovery workflow checks out the tag, downloads
the published assets, verifies `SHA256SUMS`, tests the generated formula on all
four target platforms, and updates the tap only after every test passes. It
does not rebuild or replace release assets.

Publish a new patch version when product source or a release artifact must
change. Do not move an existing tag.

If an artifact itself is defective, mark the release as withdrawn and publish
a new patch version. Checksums in a published tap commit must never be changed
without a corresponding new release.
