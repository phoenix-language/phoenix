/** Valid token types for the tokenizer */
export const Internal_TokenTypes = {
    // Data types
    STRING: "STRING", // "string"
    INT: "INT", // 1
    FLOAT: "FLOAT", // 1.1

    // ex: variables, classes, etc
    IDENTIFIER: "IDENTIFIER",
    // if statements, while statements, etc
    KEYWORD: "KEYWORD",
    // End of file
    EOF: "EOF",

    // operators
    PLUS: "PLUS", // +
    MINUS: "MINUS", // -
    MULTIPLY: "MULTIPLY", // *
    DIVIDE: "DIVIDE", // /

    // brackets
    LPAREN: "LPAREN", // (
    RPAREN: "RPAREN", // )
    LSQUARE: "LSQUARE", // [
    RSQUARE: "RSQUARE", // ]

    // Logical operators
    EE: "EE", // ==
    NE: "NE", // !=
    LT: "LT", // <
    GT: "GT", // >
    GTE: "GTE", // >=
    LTE: "LTE", // <=
    COMMA: "COMMA", // ,
    NEWLINE: "NEWLINE", // \n ;

    KEYWORDS: [
        // Variable declaration
        "define",
        ":=",

        // conditionals
        "if",
        "else",
        "|", // or
        "&", // and

        // loops
        "phor", // for loop
        "while",
        "do",
    ],
};

export const Internal_Constants = {
    numbers: "0123456789",
    letters: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
};
