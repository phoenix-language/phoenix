package parser

import (
	"github.com/phoenix-language/phoenix/runtime/ast"
	"github.com/phoenix-language/phoenix/runtime/lexer"
	tokenizer "github.com/phoenix-language/phoenix/runtime/tokens"
)

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

func (p Parser) parseStatement() interface{} {
	switch p.currentToken.Type {
	case tokenizer.DECLARE:
		return p.parseDeclareStatement()
	case tokenizer.PASS:
		return p.parsePassStatement()
	default:
		return p.parseExpressionStatement()
	}
}

func (p Parser) parseDeclareStatement() any {
	return nil
}

func (p Parser) parsePassStatement() any {
	return nil
}

func (p Parser) parseExpressionStatement() any {
	return nil
}
