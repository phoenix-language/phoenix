## language example

- [ ] C-like syntax
```text
Semi-colon ;
Comma ,
Brackets [ ]
Curly Braces { }
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
declare AdvancedFunc :: phunc(a, b) {
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
declare arr :: [1, 2, 3]

arr[0] // => 1
```
- [ ] Hash data type
```text
declare hash :: {
  "name": "Miku",
  "age": 17
}

hash["name"] // => "Miku"
```