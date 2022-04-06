package ast

type Node interface {
	// TokenLiteral Method that returns the literal value of the token its associated with.
	// This will only be used for debugging purposes.
	TokenLiteral() string
}

type Statement interface {
	Node
	statementNode()
}

type Expression interface {
	Node
	expressionNode()
}

// Program Root node of the AST.
// eg: Program.Statements[i]
type Program struct {
	Statements []Statement
}

func (p *Program) TokenLiteral() string {
	if len(p.Statements) > 0 {
		return p.Statements[0].TokenLiteral()
	} else {
		return ""
	}
}
