// AST Node for a program
pub struct Program {
    pub statements: Vec<Statement>,
}

// AST Node for statements
pub enum Statement {
    VariableDeclaration {
        identifier: String,
        value: Expression,
        datatype: Type,
    },
    FunctionDeclaration {
        name: String,
        parameters: Vec<String>,
        body: Vec<Statement>,
        return_type: Type,
    },
    ReturnStatement {
        expression: Option<Expression>,
    },
    IfStatement {
        condition: Expression,
        true_branch: Vec<Statement>,
        false_branch: Option<Vec<Statement>>,
    },
    WhileStatement {
        condition: Expression,
        body: Vec<Statement>,
    },
    ForStatement {
        variable: String,
        start: Expression,
        end: Expression,
        body: Vec<Statement>,
    },
    BreakStatement,
    ContinueStatement,
    ExpressionStatement(Box<Expression>),
}

// AST Node for expressions
pub enum Expression {
    Identifier(String),
    Literal(Literal),
    BinaryOperation {
        left: Box<Expression>,
        operator: Operator,
        right: Box<Expression>,
    },
    UnaryOperation {
        operator: Operator,
        operand: Box<Expression>,
    },
    FunctionCall {
        name: String,
        arguments: Vec<Expression>,
    },
    MatchStatement {
        value: Box<Expression>,
        cases: Vec<MatchCase>,
    },
    ArrayLiteral(Vec<Expression>),
    HashMapLiteral(Vec<(Expression, Expression)>),
}

// AST Node for match cases
pub struct MatchCase {
    pub pattern: Expression,
    pub body: Vec<Statement>,
}

// AST Node for literal values
pub enum Literal {
    Number(f64),
    String(String),
    Boolean(bool),
    Null,
}

// AST Node for types
pub enum Type {
    NumberType,
    StringType,
    BooleanType,
    VecType(Box<Type>),
    HashType(Box<Type>),
    NullType,
    AnyType,
    VoidType,
}

// AST Node for operators
pub enum Operator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Not,
    Equal,
    NotEqual,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    And,
    Or,
}
