# FalkorDB Shell

An unofficial command line client for [FalkorDB](https://www.falkordb.com/).

Install by running

```shell
$ cargo install falkordb-shell
```

Then start it by running

```
$ falkordb-shell --graph Shell
```

This will start the command line client and automatically connect to a graph called 'Shell'.

## Features

The history of commands is saved in `$HOME/.falkordb_shell_history`.

Use `.tutorial` inside the shell to run the interactive tutorial in a dedicated `tutorial` graph. Press ENTER to run the current step, then press ENTER again to reveal the next step. If you run another statement during the tutorial, the shell shows just that result and waits until ENTER to reveal the pending tutorial step.

Use `.delete NAME` to review the node and edge counts of a graph, confirm the action, and then delete that graph.

Use `.list` to show every graph together with its node and edge counts.
