## What this changes

<!-- One or two sentences. Why, not just what. -->

## Checklist

- [ ] `just ci` passes
- [ ] Comments explain *why* where the reasoning is not obvious
- [ ] Commit messages follow Conventional Commits

If this adds or changes a typed field:

- [ ] Both `from_object` and `to_json` updated in `codec.rs`
- [ ] The member added to the `COMPLETE` fixture in that file's tests

If this adds or changes a validation rule:

- [ ] At least one `corpus/valid/` case and one `corpus/invalid/` case demonstrate it
- [ ] Both are registered in `corpus/manifest.json`

If this touches the specification:

- [ ] A discussion issue exists and is linked below
- [ ] `spec/v1/` is untouched — published versions are never edited in place
- [ ] Mechanical transformations are added to `migrations/` as declarative manifests

If this changes the CLI's `--json` output or the C ABI:

- [ ] The change is additive, or `reportVersion` / the ABI version is bumped
- [ ] `docs/cli.md` is updated

## Related issues

<!-- Fixes #123 -->
