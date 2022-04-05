package lexer

import (
	tokenizer "github.com/phoenix-language/phoenix/runtime/tokens"
	"testing"
)

func TestNextToken(t *testing.T) {
	input := `
declare six = 5;
`

	test := []struct {
		expectedType    tokenizer.TokenType
		expectedLiteral string
	}{
		{tokenizer.DECLARE, "declare"},
		{tokenizer.IDENT, "six"},
		{tokenizer.ASSIGN, "="},
		{tokenizer.INT, "5"},
		{tokenizer.SEMICOLON, ";"},
		{tokenizer.EOF, ""},
	}

	l := LexNewChar(input)

	for i, tt := range test {
		tok := l.LexNextToken()

		if tok.Type != tt.expectedType {
			t.Fatalf("tests[%d] - tokentype wrong. expected=%q, got=%q", i, tt.expectedType, tok.Type)
		}

		if tok.Literal != tt.expectedLiteral {
			t.Fatalf("tests[%d] - literal wrong. expected=%q, got=%q", i, tt.expectedLiteral, tok.Literal)
		}
	}
}
