# Contributing

## Release and publish

* Update version number in Cargo.toml and in the comment below.
* `cargo build`
* `git add .`
* `git commit -m "update version to v0.1.4"`
* `git push`
* `git tag -a v0.1.4 -m "publish version v0.1.4"`
* `git push --tags`
* Pushing a tag starting with `v` triggers GitHub Actions to verify the tag matches `Cargo.toml`, build binaries for Linux, macOS, and Windows, publish a GitHub release with those assets, and deploy a GitHub Pages site based on `README.md`.
* `cargo publish`


## TODO

* Create a tutorial (instead of the .info ?)
    * Show a command, let the user execute it (or shall we don that?).
    * Shall we allow the user to execute other commands?
    * Shall we let the user to stop / continue the tutorial or to start from the beginning again?
* Load some plain text file.
* Dump data to some file format.
* Separate the tests to their own file.
