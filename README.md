# Phoenix Language🐦

# Warning ⚠️

Phoenix is not ready for production in any way and is still under heavy development. 
The API is subject to change without notice. Use at your own risk.

## What is this?

Phoenix is a [Concurrent language](https://en.wikipedia.org/wiki/Concurrency_(computer_science)), [Garbage collected languages](https://en.wikipedia.org/wiki/Garbage_collection_(computer_science)) for [Computers](https://en.wikipedia.org/wiki/Computer). Currently implemented with go, but in the future will be rewritten in phoenix itself!

## Why?

Why not? I'm simply interested in how programming languages work on the inside.
I also realized, that doing this in typescript was not a good idea, so I switched to
[golang](https://go.dev/) about midway through the project.

## Features

- [ ] C-like syntax
```text
Semi-colon ;
Comma ,
Brackets [ ]
Braces { }
```
- [ ] variable binding
```text
declare age :: 17
```
- [ ] integers and booleans
- [ ] arithmetic expressions
- [ ] built-in functions
- [ ] first class and higher order functions
```text
// High order functions
declare add :: phunc(a, b) => { a + b } 
// or 
declare add :: phunc(a, b) => { pass a + b }

add(1, 2) // => 3

// First class functions
declare sub :: phunc(a, b) => { a - b } 

// Functions can be passed as arguments
declare minus :: phunc(sub, a) => { pass sub - a }

```
- [ ] closures
```text
declare AdvancedFunc = phunc(a, b) {
    declare c :: a + b
    // allows for the function to call itself
    pass AdvancedFunc(a, c) {
        pass a + c / b
    }
}
```
- [ ] String data type
- [ ] Array data type
```text
declare arr = [1, 2, 3]

arr[0] // => 1
```
- [ ] Hash data type
```text
declare hash = {
  "name": "Miku",
  "age": 17
}

hash["name"] // => "Miku"
```

## Performance ⚡

soon...

## TODO list 📃

soon...


## Developers

**Coming soon**

# Contributing 🌎

Join the discord for more information: https://discord.gg/U4FmBUHzEP
