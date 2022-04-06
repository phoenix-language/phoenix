package parser

import (
	"github.com/phoenix-language/phoenix/runtime/lexer"
	tokenizer "github.com/phoenix-language/phoenix/runtime/tokens"
)

type Parser struct {
	l *lexer.Lexer

	currentToken tokenizer.Token
	peekToken    tokenizer.Token
}
