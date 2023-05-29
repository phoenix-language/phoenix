# Syntax for the Phoenix programming language

# Keywords

- `declare` (Variable declaration)
- `phunc` (Function declaration)
- `return` (Return statement)
- `if` (If statement)
- `else` (Else statement)
- `while` (While statement)
- `for` (For statement)
- `break` (Break statement)
- `continue` (Continue statement)
- `true` (Boolean true)
- `false` (Boolean false)
- `null` (Null value)
- `match` (Match statement)
- `=>` (Match statement case)
- `in` (For statement)
- `..` (For statement)
- `terminal` (Terminal object)
- `Vec` (Array)
- `Hash` (Hashmap)
- `!=` (Not equal)
- `==` (Equal)
- `>` (Greater than)
- `<` (Less than)
- `>=` (Greater than or equal)
- `<=` (Less than or equal)
- `+` (Addition)
- `-` (Subtraction)
- `*` (Multiplication)
- `/` (Division)
- `%` (Modulo)
- `!` (Not)
- `&&` (And)
- `||` (Or)

# Types

- `Number` (Number) [Can be an integer or a float]
- `Bool` (Boolean)
- `String` (String)
- `Vec` (Array)
- `Hash` (Hashmap)
- `Null` (Null value)

The syntax for types is `Identifier : Type`.

- Functions would be `phunc NAME (Params: TYPE): RETURN_TYPE { ... }`
- Variables would be `declare NAME: TYPE = VALUE;`

## Comments

Comments are written with `//`.  

## Variables assignment

Variables are assigned with the `var` keyword and the `=` operator.  

```phoenix
declare x: Number = 10;
declare y: Number = 20;
```

## Functions
```phoenix
declare num1: Number = 5;
declare num2: Number = 5;

phunc addTwoNumbers(x: Number, y: Number): number {
    return x + y;
};

declare log = phunc logTheSum(): Void {
    terminal.write(addTwoNumbers(num1, num2));
};

log();
```

## If statements
```phoenix
declare x: Number = 10;

if x == 10 {
    terminal.write("x is 10");
} else {
    terminal.write("x is not 10");
};
```

## While statements
```phoenix
declare x: Number = 0;

while x < 10 {
    terminal.write(x);
    x = x + 1;
};
```

## For statements
```phoenix
for x in 0..10 {
    terminal.write(x);
};
```

## Match statements
```phoenix
declare x: Number = 10;

match x {
    10 => terminal.write("x is 10");
    20 => terminal.write("x is 20");
    _ => terminal.write("x is not 10 or 20");
};
```

## Break and continue statements
```phoenix
declare x: Number = 0;

while x < 10 {
    if x == 5 {
        break;
    };

    terminal.write(x);
    x = x + 1;
};

for x in 0..10 {
    if x == 5 {
        continue;
    };

    terminal.write(x);
};
```

## Vec
```phoenix
declare myVec: Vec<Number> = [1, 2, 3];

vec.push(10);

terminal.write(vec[0]);

vec[0] = 40;

terminal.write(vec[0]);
```

## Hash
```phoenix
declare hash: Hash<String, Number> = {};

hash.set("x", 10);

terminal.write(hash.get("x"));
```

## Null
```phoenix
declare x: Number = null;
```

## Boolean
```phoenix
declare x: Bool = true;
declare y: Bool = false;
```

## String
```phoenix
declare x: String = "Hello world!";
```

## Number
```phoenix
declare x: Number = 10;
declare y: Number = 10.5;
```
