# Contributing

## Release and publish

* Update version number in Cargo.toml and in the comment below.
* `git add .`
* `git commit -m "update version to v0.2.0"`
* `git push`
* `git tag -a v0.2.0 -m "publish version v0.2.0"`
* `git push --tags`
* Pushing a tag starting with `v` triggers GitHub Actions to verify the tag matches `Cargo.toml`, build binaries for Linux, macOS, and Windows, and publish a GitHub release with those assets.
* `cargo publish`


## TODO

* Create a tutorial (instead of the .info ?)
    * Show a command, let the user execute it (or shall we don that?).
    * Shall we allow the user to execute other commands?
    * Shall we let the user to stop / continue the tutorial or to start from the beginning again?
* .list can it show the size of each graph? (e.g. number of nodes, number of edges?)
* List nodes (types) and their count. Lost edges (types) and their count.
* Load some plain text file.
* Dump data to some file format.
* Separate the tests to their own file.
* Add GitHub Actions to run the tests.
* Add GitHub Actions to generate executables for Linux, macOS, and Windows when we push out a tag starting with v.

