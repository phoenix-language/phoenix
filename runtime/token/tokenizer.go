package tokenizer

type Token struct {
	// The data type of the token
	TokenType int
	Lexer     string
	Literal   interface{}
	// The line number the token was found on
	Line int
}

// Creates a new token
func CreateToken(tokenType int, lexer string, literal any, line int) Token {
	return Token{TokenType: tokenType, Lexer: lexer, Literal: literal, Line: line}
}

// Returns the string representation of the token
func findType(text string) int {
	switch text {
	case "and":
		return AND
	case "else":
		return ELSE
	case "false":
		return FALSE
	case "for":
		return FOR
	case "phunc":
		return PHUNC
	case "if":
		return IF
	case "nil":
		return NIL
	case "or":
		return OR
	case "terminal":
		return TERMINAL
	case "return":
		return RETURN
	case "true":
		return TRUE
	case "define":
		return DEFINE
	case "while":
		return WHILE
	default:
		return IDENTIFIER
	}
}
