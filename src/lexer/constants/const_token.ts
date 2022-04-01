export const Internal_TokeTypes = {
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
    ARROW: "ARROW", // =>
    EE: "EE", // ==
    NE: "NE", // !=
    LT: "LT", // <
    GT: "GT", // >
    GTE: "GTE", // >=
    LTE: "LTE", // <=
    COMMA: "COMMA", // ,
    LSQUARE: "LSQUARE", // [
    RSQUARE: "RSQUARE", // ]
    NEWLINE: "NEWLINE", // \n ;

    KEYWORDS: [
        "define",
        "if",
        "else",
        "while",
        "do",
        "::",
        "|",
        "&",
    ],
};

export const Internal_Constants = {
    numbers: "0123456789",
    letters: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
};
