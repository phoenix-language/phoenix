package parser

import (
	"github.com/phoenix-language/phoenix/runtime/ast"
	"github.com/phoenix-language/phoenix/runtime/lexer"
	tokenizer "github.com/phoenix-language/phoenix/runtime/tokens"
)

type Parser struct {
	// pointer instance for lexer package
	l *lexer.Lexer
	// stacks errors hit during parsing
	errors []string

	currentToken tokenizer.Token
	peekToken    tokenizer.Token
}

func Create(l *lexer.Lexer) *Parser {
	p := &Parser{
		l: l,
	}
	// Read the next two tokens, and peak ahead one
	p.nextToken()
	p.nextToken()

	return p
}

// Check the next token in the parser queue
func (p Parser) nextToken() {
	p.currentToken = p.peekToken
	p.peekToken = p.l.LexNextToken()
}

func (p *Parser) ParseProgram() *ast.Program {
	program := &ast.Program{}
	program.Statements = []ast.Statement{}

	for p.currentToken.Type != tokenizer.EOF {
		stmt := p.parseStatement()
		if stmt != nil {
			program.Statements = append(program.Statements, stmt)
		}
		p.nextToken()
	}

	return program
}

func (p Parser) parseDeclareStatement() ast.Statement {
	stmt := &ast.DeclareStatement{
		Token: p.currentToken,
	}

	if !p.expectPeek(tokenizer.IDENT) {
		return nil
	}

	stmt.Name = &ast.Identifier{
		Token: p.currentToken,
		Value: p.currentToken.Literal,
	}

	if !p.expectPeek(tokenizer.ASSIGN) {
		return nil
	}

	for !p.currentTokenIs(tokenizer.SEMICOLON) {
		p.nextToken()
	}

	return stmt

}

func (p Parser) parseStatement() ast.Statement {
	switch p.currentToken.Type {
	case tokenizer.DECLARE:
		return p.parseDeclareStatement()
	default:
		return nil
	}
}

// Enforces correct order of tokens by checking the next token type
// If the token type is correct, it returns true, otherwise it returns false
// True will advance the parser to the next token
func (p Parser) expectPeek(ident tokenizer.TokenType) bool {
	if p.currentTokenIs(ident) {
		p.nextToken()
		return true
	} else {
		return false
	}
}

func (p Parser) currentTokenIs(t tokenizer.TokenType) bool {
	return p.currentToken.Type == t
}
