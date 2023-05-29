#[derive(Debug, PartialEq, Clone)]
pub struct Program {
    pub statements: Vec<Statement>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Statement {
    VariableDeclaration {
        identifier: String,
        value: Expression,
        datatype: Type,
    },
    VariableAssignment {
        identifier: String,
        value: Expression,
    },
    FunctionDeclaration {
        visibility: Visibility,
        name: String,
        parameters: Vec<FunctionParameter>,
        return_type: Type,
        body: Vec<Statement>,
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

#[derive(Debug, PartialEq, Clone)]
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

#[derive(Debug, PartialEq, Clone)]
pub struct MatchCase {
    pub pattern: Expression,
    pub body: Vec<Statement>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Literal {
    Number(f64),
    String(String),
    Boolean(bool),
    Null,
}

#[derive(Debug, PartialEq, Clone)]
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


#[derive(Debug, PartialEq, Clone)]
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

#[derive(Debug, PartialEq, Clone)]
pub struct FunctionParameter {
    pub identifier: String,
    pub datatype: Type,
    pub default_value: Option<Expression>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Visibility {
    Public,
    Private,
    // ... other visibility modifiers ...
}
