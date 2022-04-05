package lexer

import (
	tokenizer "github.com/phoenix-language/phoenix/runtime/tokens"
)

type Lexer struct {
	input        string // the string being scanned
	position     int    // current position in input (points to current char)
	readPosition int    // current reading position in input (after current char)
	ch           byte   // current char under examination
}

// LexNewChar Accepts a string and returns a new Lexer with that string as input
func LexNewChar(input string) *Lexer {
	l := &Lexer{input: input}
	l.lexReadChar()
	return l
}

// The purpose of lexReadChar is to give us the next character and advance our position in the input string.
func (l *Lexer) lexReadChar() {
	if l.readPosition >= len(l.input) {
		l.ch = 0
	} else {
		l.ch = l.input[l.readPosition]
	}
	l.position = l.readPosition
	l.readPosition += 1
}

// LexNextToken returns the next token in the string and advances our position in the input string.
func (l *Lexer) LexNextToken() tokenizer.Token {
	var tok tokenizer.Token

	l.lexIgnoreWhitespace()

	switch l.ch {
	case '=':
		tok = LexNewToken(tokenizer.ASSIGN, l.ch)
	case ';':
		tok = LexNewToken(tokenizer.SEMICOLON, l.ch)
	case '(':
		tok = LexNewToken(tokenizer.LPAREN, l.ch)
	case ')':
		tok = LexNewToken(tokenizer.RPAREN, l.ch)
	case ',':
		tok = LexNewToken(tokenizer.COMMA, l.ch)
	case '+':
		tok = LexNewToken(tokenizer.PLUS, l.ch)
	case '{':
		tok = LexNewToken(tokenizer.LBRACE, l.ch)
	case '}':
		tok = LexNewToken(tokenizer.RBRACE, l.ch)
	case '-':
		tok = LexNewToken(tokenizer.MINUS, l.ch)
	case '!':
		tok = LexNewToken(tokenizer.BANG, l.ch)
	case '*':
		tok = LexNewToken(tokenizer.ASTERISK, l.ch)
	case '/':
		tok = LexNewToken(tokenizer.SLASH, l.ch)
	case '<':
		tok = LexNewToken(tokenizer.LT, l.ch)
	case '>':
		tok = LexNewToken(tokenizer.GT, l.ch)
	case 0:
		tok.Literal = ""
		tok.Type = tokenizer.EOF

	default:
		if isLetter(l.ch) {
			tok.Literal = l.LexReadIdentifier()
			tok.Type = tokenizer.LookupIdent(tok.Literal)
			return tok
		} else if isDigit(l.ch) {
			tok.Type = tokenizer.INT
			tok.Literal = l.LexReadNumber()
			return tok
		} else {
			tok = LexNewToken(tokenizer.ILLEGAL, l.ch)
		}
	}

	l.lexReadChar()
	return tok
}

// LexReadIdentifier reads an identifier from the input string
func (l *Lexer) LexReadIdentifier() string {
	position := l.position
	for isLetter(l.ch) {
		l.lexReadChar()
	}
	return l.input[position:l.position]
}

// LexNewToken creates a new token with the given type and literal
func LexNewToken(tokenType tokenizer.TokenType, ch byte) tokenizer.Token {
	return tokenizer.Token{Type: tokenType, Literal: string(ch)}
}

// LexIgnoreWhitespace skips over whitespaces in the lexer input string
func (l *Lexer) lexIgnoreWhitespace() {
	for l.ch == ' ' || l.ch == '\t' || l.ch == '\n' || l.ch == '\r' {
		l.lexReadChar()
	}
}

// LexReadNumber returns the next sequence of digits in the input string
func (l *Lexer) LexReadNumber() string {
	position := l.position
	for isDigit(l.ch) {
		l.lexReadChar()
	}
	return l.input[position:l.position]
}

// isDigit returns true if the input character is a digit
func isDigit(ch byte) bool {
	return '0' <= ch && ch <= '9'
}

// isLetter returns true if the input character is a letter
func isLetter(ch byte) bool {
	return 'a' <= ch && ch <= 'z' || 'A' <= ch && ch <= 'Z' || ch == '_'
}
