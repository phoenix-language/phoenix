package tokenizer

type TokenType string

type Token struct {
	// Type of the token (e.g. "Identifier", "Number", etc.)
	Type TokenType
	// The literal value of the token.
	Literal string
}

const (
	ILLEGAL = "ILLEGAL" // Illegal token
	EOF     = "EOF"     // End of file

	IDENT = "IDENT" // add, foobar, x, y, ...
	INT   = "INT"   // 1343456

	ASSIGN = "::" // assignment operator
	PLUS   = "+"

	COMMA     = ","
	SEMICOLON = ";"
	LPAREN    = "("
	RPAREN    = ")"
	LBRACE    = "{"
	RBRACE    = "}"

	PHUNC   = "PHUNC"   // functions
	DECLARE = "DECLARE" // variables
)
