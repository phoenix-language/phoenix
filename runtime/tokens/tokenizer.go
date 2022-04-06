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

	// Identifiers + Literals
	IDENT  = "IDENT"  // add, foobar, x, y, ...
	INT    = "INT"    // 1343456
	STRING = "STRING" // "foobar"

	// Operators
	ASSIGN   = "::"
	PLUS     = "+"
	MINUS    = "-"
	BANG     = "!"
	ASTERISK = "*"

	SLASH = "/"
	LT    = "<"
	GT    = ">"
	EQ    = "=="
	NotEq = "!="

	// Delimiters
	COMMA     = ","
	SEMICOLON = ";"
	LPAREN    = "("
	RPAREN    = ")"
	LBRACE    = "{"
	RBRACE    = "}"

	// Keywords
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
