# How to Contribute

## About New Code

As of writing, feldspar is in its very early stages, but it is based on several
functional prototypes; some previous solutions will be preserved and some will
be replaced (see
[building-blocks](https://github.com/bonsairobo/building-blocks)). There also
happens to be a lot of context that is not always represented in the source code
or documentation.

**I don't want anyone's hard work, time, and energy to go to waste as a result
of a technical disagreement**. As such, if you would like to contribute, please
understand that you are most likely to have a successful PR if you talk to me
(@bonsairobo) on [Discord](https://discord.com/invite/CnTNjwb) in the
`#feldspar-dev` channel about the scope of your contribution. Features rarely
live in a vacuum. Assume I have strong opinions about something you want to
touch, and let's discuss.

## Onboarding

- Please at least read the automatically generated documentation on the main
  branch with `cargo doc` in each relevant crate (in the `crates/` directory).
  Peruse the types and get a lay of the land.
- If you want to understand the code structure, start reading the `lib.rs` of
  each crate, starting with `feldspar-map`, which is the most critical
  underlying infrastructure of any `feldspar` solution.

## Out of Scope Stuff (FAQ)

Please don't make any issues or PRs about this stuff. These are basically edicts
from me based on my current opinions about the core architecture of the voxel
engine that *I need for my own video game*. This list is subject to change as
the feldspar project evolves. Feel free to chat about it on the feldspar Discord
though.

- Generic voxel data types
  - Annoying to support/maintain, sorry! Let's not constrain ourselves with
    abstractions too early.
- Ray-tracing renderers
  - Might be a fun thing to try eventually! I'm not a rendering expert.
  - I would hope that it's possible for anyone to write a custom rendering
    plugin on top of `feldspar-map`, but I don't have plans to lead the effort
    on a ray-tracing renderer.
- Voxel physics engines
  - I don't expect to need realistic physics for my own game (primarily in the
    strategy genre).
  - Maybe you can build your own physics on top of feldspar. But I won't
    sidetrack the core data structures to optimize for physics.

## Hot Tips About Legacy Code

- `building-blocks` should be considered deprecated for the purpose of this
  project, but many of the ideas therein have been preserved!
- Anything in the `archived/` directory is basically deprecated, but some useful code might get copied out of there until it is
  replaced entirely.
- Eventually I want `feldspar-editor` to move into the `feldspar` repo as a
  top-level executable.
