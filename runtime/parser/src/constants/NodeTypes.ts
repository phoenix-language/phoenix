export const NodeTypes = {
    AdditionExpression: "AdditionExpression",
    MultiplicationExpression: "MultiplicationExpression",

    PrimaryExpression: "PrimaryExpression",
    ParanthesizedExpression: "ParanthesizedExpression",

    IdentifierExpression: "IdentifierExpression",
    AssignmentExpression: "AssignmentExpression",

    Program: "Program",
    BinaryExpression: "BinaryExpression",

    LogicalExpression: "LogicalExpression",
    LogicalANDExpression: "LogicalANDExpression",
    LogicalORExpression: "LogicalORExpression",

    RelationalExpression: "RelationalExpression",
    EqualityExpression: "EqualityExpression",

    BlockStatement: "BlockStatement",
    EmptyStatement: "EmptyStatement",
    ExpressionStatement: "ExpressionStatement",
    InitStatement: "InitStatement",
    PrintStatement: "PrintStatement",

    IfStatement: "IfStatement",
    WhileStatement: "WhileStatement",
    BreakStatement: "BreakStatement",
    ContinueStatement: "ContinueStatement",

    VariableStatement: "VariableStatement",
    VariableDeclaration: "VariableDeclaration",

    BooleanLiteral: "BooleanLiteral",
    NumericLiteral: "NumericLiteral",
    StringLiteral: "StringLiteral",
    NullLiteral: "NullLiteral",
} as const;
