package lexer

import (
	tokenizer "github.com/phoenix-language/phoenix/runtime/tokens"
	"testing"
)

func TestNextToken(t *testing.T) {
	input := `
declare six :: 5;
`

	test := []struct {
		expectedType    tokenizer.TokenType
		expectedLiteral string
	}{
		{tokenizer.DECLARE, "declare"},
		{tokenizer.IDENT, "six"},
		{tokenizer.ASSIGN, "::"},
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

// Testing mathematical operators
func TestMathTokens(t *testing.T) {
	input := `
declare six :: 5;
declare seven :: 6;

declare add :: six + seven;
declare sub :: six - seven;
declare mul :: six * seven;
declare div :: six / seven;
`

	test := []struct {
		expectedType    tokenizer.TokenType
		expectedLiteral string
	}{
		{tokenizer.DECLARE, "declare"},
		{tokenizer.IDENT, "six"},
		{tokenizer.ASSIGN, "::"},
		{tokenizer.INT, "5"},
		{tokenizer.SEMICOLON, ";"},
		{tokenizer.DECLARE, "declare"},
		{tokenizer.IDENT, "seven"},
		{tokenizer.ASSIGN, "::"},
		{tokenizer.INT, "6"},
		{tokenizer.SEMICOLON, ";"},
		{tokenizer.DECLARE, "declare"},
		{tokenizer.IDENT, "add"},
		{tokenizer.ASSIGN, "::"},
		{tokenizer.IDENT, "six"},
		{tokenizer.PLUS, "+"},
		{tokenizer.IDENT, "seven"},
		{tokenizer.SEMICOLON, ";"},
		{tokenizer.DECLARE, "declare"},
		{tokenizer.IDENT, "sub"},
		{tokenizer.ASSIGN, "::"},
		{tokenizer.IDENT, "six"},
		{tokenizer.MINUS, "-"},
		{tokenizer.IDENT, "seven"},
		{tokenizer.SEMICOLON, ";"},
		{tokenizer.DECLARE, "declare"},
		{tokenizer.IDENT, "mul"},
		{tokenizer.ASSIGN, "::"},
		{tokenizer.IDENT, "six"},
		{tokenizer.ASTERISK, "*"},
		{tokenizer.IDENT, "seven"},
		{tokenizer.SEMICOLON, ";"},
		{tokenizer.DECLARE, "declare"},
		{tokenizer.IDENT, "div"},
		{tokenizer.ASSIGN, "::"},
		{tokenizer.IDENT, "six"},
		{tokenizer.SLASH, "/"},
		{tokenizer.IDENT, "seven"},
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

// Checking ==, !=, pass, true, false
func TestNextToken3(t *testing.T) {
	input := `
	if (5 < 10) {
		pass true;
	} else {
		pass false;
	}
	10 == 10;
	10 != 9;
`

	test := []struct {
		expectedType    tokenizer.TokenType
		expectedLiteral string
	}{
		{tokenizer.IF, "if"},
		{tokenizer.LPAREN, "("},
		{tokenizer.INT, "5"},
		{tokenizer.LT, "<"},
		{tokenizer.INT, "10"},
		{tokenizer.RPAREN, ")"},
		{tokenizer.LBRACE, "{"},
		{tokenizer.PASS, "pass"},
		{tokenizer.TRUE, "true"},
		{tokenizer.SEMICOLON, ";"},
		{tokenizer.RBRACE, "}"},
		{tokenizer.ELSE, "else"},
		{tokenizer.LBRACE, "{"},
		{tokenizer.PASS, "pass"},
		{tokenizer.FALSE, "false"},
		{tokenizer.SEMICOLON, ";"},
		{tokenizer.RBRACE, "}"},
		{tokenizer.INT, "10"},
		{tokenizer.EQ, "=="},
		{tokenizer.INT, "10"},
		{tokenizer.SEMICOLON, ";"},
		{tokenizer.INT, "10"},
		{tokenizer.NotEq, "!="},
		{tokenizer.INT, "9"},
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
