/** Valid token types for our parser and lexer */
export const TokenTypes = {
    // Basic data types
    NULL_TYPE: null, // No object value
    STRING_TYPE: "STRING", // "string"
    NUMBER_TYPE: "NUMBER", // 10 OR 4.3
    BOOLEAN_TYPE: "BOOLEAN", // TRUE or FALSE
    LOGIC_AND_TYPE: "LOGIC_AND", // &&
    LOGIC_OR_TYPE: "LOGIC_OR", // ||

    // MISC
    ASSIGN_TYPE: "ASSIGN",
    ADVANCED_ASSIGN: "ADVANCED_ASSIGN",
    IDENTIFIER_TYPE: "IDENTIFIER",

    // Symbols
    SEMI_COLON_TYPE: ";",
    COMMA_TYPE: ",",
    OPEN_CURLY_BRACE_TYPE: "{",
    CLOSED_CURLY_BRACE_TYPE: "}",
    OPEN_PARENTHESIS_TYPE: "(",
    CLOSED_PARENTHESIS_TYPE: ")",

    // Operators
    ADDITION_OPERATOR_TYPE: "ADDITIVE_OPERATOR",
    MULTIPLICATION_OPERATOR_TYPE: "MULTIPLICATIVE_OPERATOR",
    RELATIONAL_OPERATOR: "RELATIONAL_OPERATOR",
    EQUALITY_OPERATOR: "EQUALITY_OPERATOR",

    // Built in with js
    SYSTEM_TERMINAL_TYPE: "system terminal", // Prints something to the console

    // Conditionals
    IF_TYPE: "if",
    ELSE_TYPE: "else",
    ELSE_IF_TYPE: "else if",
} as const;

export type ParserSpecs = typeof _ParserSpecs;

export const _ParserSpecs = [
    // White spaces
    { regex: /^\s+/, tokenType: TokenTypes.NULL_TYPE },

    // single line Comments
    { regex: /^\/\/.*/, tokenType: TokenTypes.NULL_TYPE },
    // multi line comments
    { regex: /^\/\*[\s\S]*?\*\//, tokenType: TokenTypes.NULL_TYPE },

    // Symbols, delimiters
    { regex: /^;/, tokenType: TokenTypes.SEMI_COLON_TYPE },
    { regex: /^\{/, tokenType: TokenTypes.OPEN_CURLY_BRACE_TYPE },
    { regex: /^\}/, tokenType: TokenTypes.CLOSED_CURLY_BRACE_TYPE },
    { regex: /^\(/, tokenType: TokenTypes.OPEN_PARENTHESIS_TYPE },
    { regex: /^\)/, tokenType: TokenTypes.CLOSED_PARENTHESIS_TYPE },
    { regex: /^,/, tokenType: TokenTypes.COMMA_TYPE },

    // Number
    { regex: /^-?\d+/, tokenType: TokenTypes.NUMBER_TYPE },

    // Boolean
    { regex: /^\true\b/, tokenType: TokenTypes.BOOLEAN_TYPE },
    { regex: /^\false\b/, tokenType: TokenTypes.BOOLEAN_TYPE },

    // Keywords
    { regex: /^\define\b/, tokenType: TokenTypes.ASSIGN_TYPE },
    { regex: /^\if\b/, tokenType: TokenTypes.IF_TYPE },
    { regex: /^\else\b/, tokenType: TokenTypes.ELSE_TYPE },
    { regex: /^\else\s+if\b/, tokenType: TokenTypes.ELSE_IF_TYPE },

    // Identifier
    { regex: /^\w+/, tokenType: TokenTypes.IDENTIFIER_TYPE },

    // Equality operator: ==, !=
    { regex: /^[=!]=/, tokenType: TokenTypes.EQUALITY_OPERATOR },

    // Assignment operators: :=, *=, /=, +=, -=
    { regex: /^:=/, tokenType: TokenTypes.ASSIGN_TYPE },
    { regex: /^[\*\%\/\+\-]=/, tokenType: TokenTypes.ADVANCED_ASSIGN },

    // operator
    { regex: /^[+\-]/, tokenType: TokenTypes.ADDITION_OPERATOR_TYPE },
    { regex: /^[*\/\%]/, tokenType: TokenTypes.MULTIPLICATION_OPERATOR_TYPE },
    { regex: /^[><]=?/, tokenType: TokenTypes.RELATIONAL_OPERATOR },

    // logical operators: &&, ||
    { regex: /^&&/, tokenType: TokenTypes.LOGIC_AND_TYPE },
    { regex: /^\|\|/, tokenType: TokenTypes.LOGIC_OR_TYPE },

    // String
    { regex: /^"[^"]*"/, tokenType: TokenTypes.STRING_TYPE },
    { regex: /^'[^']*'/, tokenType: TokenTypes.STRING_TYPE },
];
