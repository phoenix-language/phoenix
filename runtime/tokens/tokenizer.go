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

	ASSIGN   = "::" // assignment operator
	PLUS     = "+"
	MINUS    = "-"
	BANG     = "!"
	ASTERISK = "*"

	SLASH  = "/"
	LT     = "<"
	GT     = ">"
	EQ     = "=="
	NOT_EQ = "!="

	COMMA     = ","
	SEMICOLON = ";"
	LPAREN    = "("
	RPAREN    = ")"
	LBRACE    = "{"
	RBRACE    = "}"

	PHUNC   = "PHUNC"   // functions
	DECLARE = "DECLARE" // variables
	PASS    = "PASS"    // pass a value back from a function
	IF      = "IF"
	ELSE    = "ELSE"
	TRUE    = "TRUE"
	FALSE   = "FALSE"
)

var keywords = map[string]TokenType{
	"phunc":   PHUNC,
	"declare": DECLARE,
	"pass":    PASS,
	"if":      IF,
	"else":    ELSE,
	"true":    TRUE,
	"false":   FALSE,
}

// LookupIdent checks if the token is a keyword
func LookupIdent(ident string) TokenType {
	if tok, ok := keywords[ident]; ok {
		return tok
	}
	return IDENT
}
