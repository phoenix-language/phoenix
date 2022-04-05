package lexer

import (
	tokenizer "github.com/phoenix-language/phoenix/runtime/tokens"
	"testing"
)

func TestNextToken(t *testing.T) {
	input := `
declare five :: 5;
declare ten :: 10;

declare add :: phunc(x, y)  {
	x + y
};

declare result :: add(five, ten);
`

	test := []struct {
		expectedType    tokenizer.TokenType
		expectedLiteral string
	}{
		{tokenizer.DECLARE, "declare"},
		{tokenizer.IDENT, "five"},
		{tokenizer.ASSIGN, "::"},
		{tokenizer.INT, "5"},
		{tokenizer.SEMICOLON, ";"},
		{tokenizer.DECLARE, "declare"},
		{tokenizer.IDENT, "ten"},
		{tokenizer.ASSIGN, "::"},
		{tokenizer.INT, "10"},
		{tokenizer.SEMICOLON, ";"},
		{tokenizer.DECLARE, "declare"},
		{tokenizer.IDENT, "add"},
		{tokenizer.ASSIGN, "::"},
		{tokenizer.PHUNC, "phunc"},
		{tokenizer.LPAREN, "("},
		{tokenizer.IDENT, "x"},
		{tokenizer.COMMA, ","},
		{tokenizer.IDENT, "y"},
		{tokenizer.RPAREN, ")"},
		{tokenizer.LBRACE, "{"},
		{tokenizer.IDENT, "x"},
		{tokenizer.PLUS, "+"},
		{tokenizer.IDENT, "y"},
		{tokenizer.SEMICOLON, ";"},
		{tokenizer.RBRACE, "}"},
		{tokenizer.SEMICOLON, ";"},
		{tokenizer.DECLARE, "declare"},
		{tokenizer.IDENT, "result"},
		{tokenizer.ASSIGN, "::"},
		{tokenizer.IDENT, "add"},
		{tokenizer.LPAREN, "("},
		{tokenizer.IDENT, "five"},
		{tokenizer.COMMA, ","},
		{tokenizer.IDENT, "ten"},
		{tokenizer.RPAREN, ")"},
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
