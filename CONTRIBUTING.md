# Contributing

## Release and publish

* Update version number in Cargo.toml and in the comment below.
* `git add .`
* `git commit -m "update version to v0.2.0"`
* `git push`
* `git tag -a v0.2.0 -m "publish version v0.2.0"`
* `git push --tags`
* `cargo publish`


## TODO

* Create a tutorial (instead of the .info ?)
    * Show a command, let the user execute it (or shall we don that?).
    * Shall we allow the user to execute other commands?
    * Shall we let the user to stop / continue the tutorial or to start from the beginning again?
* .stats Show some stats about the current graph.
* .list can it show the size of each graph? (e.g. number of nodes, number of edges?)
* List nodes (types) and their count. Lost edges (types) and their count.

