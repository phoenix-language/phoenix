package runtime_tokenizer

const (
	// single-character tokens
	LEFT_PAREN  = -1  // (
	RIGHT_PAREN = -2  // )
	LEFT_BRACE  = -3  // {
	RIGHT_BRACE = -4  // }
	COMMA       = -5  // ,
	DOT         = -6  // .
	MINUS       = -7  // -
	PLUS        = -8  // +
	SEMICOLON   = -9  // ;
	SLASH       = -10 // /
	STAR        = -11 // *

	// one or two-character tokens
	BANG          = -12 // !
	BANG_EQUAL    = -13 // !=
	EQUAL         = -14 // =
	EQUAL_EQUAL   = -15 // ==
	GREATER       = -16 // >
	GREATER_EQUAL = -17 // >=
	LESS          = -18 // <
	LESS_EQUAL    = -19 // <=

	// literals
	IDENTIFIER = -20 // identifier token
	STRING     = -21 // string token
	NUMBER     = -22 // number token

	// keywords
	AND      = -23 // Local operator and token
	CLASS    = -24
	ELSE     = -25 // Control flow else token
	FALSE    = -26 // bool
	PHUNC    = -27 // Function declaration token
	FOR      = -28 // For loop token
	IF       = -29 // Control Flow if token
	NIL      = -30
	OR       = -31 // Logic operator or token
	TERMINAL = -32 // Console output token
	RETURN   = -33 // Data return token for function
	SUPER    = -34
	THIS     = -35
	TRUE     = -36 // Bool true token
	DEFINE   = -37 // Variable declaration token
	WHILE    = -38 // While loop token

	EOF = -39
)

var keywords = map[string]int{
	"&&":       AND,
	"class":    CLASS,
	"else":     ELSE,
	"false":    FALSE,
	"for":      FOR,
	"phunc":    PHUNC,
	"if":       IF,
	"nil":      NIL,
	"||":       OR,
	"terminal": TERMINAL,
	"return":   RETURN,
	"super":    SUPER,
	"this":     THIS,
	"true":     TRUE,
	"define":   DEFINE,
	"while":    WHILE,
}
