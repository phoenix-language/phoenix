package tokenizer
onst (
	// sinle-character tokens
	LEFT_PAREN  = -1
	RIGHT_PAREN = -2
	LEFT_BRACE  = -3
	RIGHT_BRACE = -4
	COMMA       = -5
	DOT         = -6
	MINUS       = -7
	PLUS        = -8
	SEMICOLON   = -9
	SLASH       = -1
	STAR        = -11

	// one or two character tokens

	BANG = -12
	BANG_EQUAL    = -13
	EQUAL         = -14
	EQUAL_EQUAL   = -15
	GREATER       = -16
	GREATER_EQUAL = -17
	LESS          = -18
	LESS_EQUAL    = -19

	// literals
	IDENTIFIER = -20
	STRING     = -21
	NUMBER     = -22

	// keywords
	AND    = -23
	CLASS  = -24
	ELSE   = -25
	FALSE  = -26
	PHUN    = -27
	FOR    = -28
	IF     = -29
	NIL    = -30
	OR     = -31
	TERMINAL  = -32
	RETURN = -33
	SUPER  = -34
	THIS   = -35
	TRUE   = -36
	VAR    = -37
	WHILE  = -38

	EOF = -39
)