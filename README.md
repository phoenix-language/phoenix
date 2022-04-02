# Phoenix Language🐦

## What is this?

Phoenix is a `static typed` language built using Typescript and the Deno
runtime. While re-imagining programming I've realized there is no need for this
language, however, we could all still be using ASM in 2022 so your welcome.

## Why?

Why not? I'm simply interested in how programming languages work on the inside.
I also realized, Javascript was the only language I know the best, and instead
of spending 6 months learning pointers in **C** I decided to use a language im
already good at.

## Performance

A concern with many dynamic programming languages is the Performance verses
standard compiled ones. Phoenix leverages the Deno runtime (built with rust) to
compile your code down to a single executable. This makes the preform loss in
using a dynamic language like javascript virtually meaning less at runtime.

## TODO list

A list of current mandatory things I need to get done for the language to work.

**CLI**

- [ ] Commands
  - [x] Help
  - [x] Discord
  - [ ] Version
  - [ ] Documentation
  - [ ] Switch to a more modern cmd framework...
- [ ] Execute nodejs scripts
- [ ] Execute deno.land scripts

**Lexer**

- [ ] Tokenizer
- [ ] Token Types

**Parser**

- [ ] Parser

**Exception**

- [x] Base class
- [ ] char exception
- [ ] type exception

**Compiler / Interrupter**

- [ ] n/a

**Grammar**

- [ ] Syntax
- [ ] Keywords

**Miscellaneous**

- [ ] standard library
- [ ] built in code formatter
- [ ] built in linter
- [ ] configuration file

## Developers

To create a development environment to build Phoenix you need a few tools.

- The [deno.land](https://deno.land/) runtime.
- [Trex](https://github.com/crewdevio/Trex) (A package manager for deno).

After installing these, you can set up deno in your code editor.

All the commands for Trex can be found in the [run.json](./run.json) file. Eg:
`trex run <command>`

_more information coming as more of the lang is development._

# Contributing

Join the discord for more information: https://discord.gg/U4FmBUHzEP
