package parser

import (
	tokenizer "github.com/phoenix-language/phoenix/runtime/tokens"
)

// Parses the next token and checks if it has any errors
func (p *Parser) peekError(t tokenizer.TokenType) {
	msg := "expected next token to be " + t.String() + ", got " + p.peekToken.Type.String() + " instead"
	p.errors = append(p.errors, msg)
}

func (p Parser) Errors() []string {
	return p.errors
}
