# tma: The tmux Automator

_tma_ is defined not so much by what I *want* in a tmux automation solution, but more by what I *don't*. To wit:

 * I don't want a full interpreted programming language. Instead, give me a single binary I can scp to remote servers or compile for just about anything.
 * I don't want a full programming language in my automation specifications. I should be able to bang something out from memory to open a few windows and panes, and set them up like I want. [TOML](https://github.com/toml-lang/toml) should work nicely.
 * I don't want a bunch of project-specific settings in my automation, particularly when I'll likely copy the same setup to every Node/Rust/Elixir project I start. Why should my tmux automation care about a session name when it can probably infer it from the current directory? Likewise, why do I need to specify a root directory? I should be able to give someone else my setup and have it run unmodified for them.
 * I don't want my automation solution to do more than automate tmux. Don't manage a directory of configurations, spawn my editor, or do things other than rock at automating tmux.

So, here's _tma_.

## Building

You'll need a [Rust toolchain installed](https://rustup.rs). With that in place, simply check out this repository and run:

```
$ cargo install
```

You'll end up with _$HOME/.cargo/bin/tma_.

## Configuring

_tma_ looks for its configuration in _.tma.toml_ in the current directory. Use the `-c` command line switch to specify a different filename.

Here is a full commented example of all currently-supported options:

```
name = "myproject" # optional, tmux session name, defaults to current directory name if unset
root = "src" # optional, relative path from which all tmux commands are executed, defaults to current directory
attach = true # optional, indicates whether or not to attach to the created session, true by default

[[window]]
name = "code" # optional window named
root = "code" # optional, path at which this window is open, relative from the session root

[[window.pane]]
root = "subdir" # optional, path at which this pane is open, relative from the session and window root
command = "vim" # optional, command run in this pane
split = "horizontal" # optional, splits this window horizontally, all other values ignored
...
```

## Example

Here is the _.tma.toml_ file I use while developing this project:

```
[[window]]
name = "code"

[[window.pane]]
command = "vim"

[[window.pane]]
command = "cargo watch"
```

This opens a tmux session with Vim on top and `cargo watch` below.

## License

Copyright (c) 2017 Nolan Darilek

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
