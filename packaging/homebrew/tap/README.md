# Schwick3 Homebrew Tap

This tap distributes public releases of
[Vibedoc](https://github.com/Schwick3/vibedoc).

Install the Vibedoc CLI and the TypeScript adapter:

```sh
brew install Schwick3/tap/vibedoc
```

The TypeScript adapter is built as a separate checksummed release artifact and
installed as the default adapter by the `vibedoc` formula. This preserves the
external-adapter architecture without requiring users to trust or install a
second formula.

The formula in this repository is generated from checksummed Vibedoc release
artifacts. Changes are published automatically after the release workflow
passes its macOS and Linux installation tests.
