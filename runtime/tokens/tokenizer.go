package tokenizer

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
