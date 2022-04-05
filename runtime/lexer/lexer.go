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

// LexNextToken returns the next token from the input string
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

func (l *Lexer) LexReadIdentifier() string {
	position := l.position
	for isLetter(l.ch) {
		l.lexReadChar()
	}
	return l.input[position:l.position]
}

func LexNewToken(tokenType tokenizer.TokenType, ch byte) tokenizer.Token {
	return tokenizer.Token{Type: tokenType, Literal: string(ch)}
}

// filters out whitespace
func (l *Lexer) lexIgnoreWhitespace() {
	for l.ch == ' ' || l.ch == '\t' || l.ch == '\n' || l.ch == '\r' {
		l.lexReadChar()
	}
}

func (l *Lexer) LexReadNumber() string {
	position := l.position
	for isDigit(l.ch) {
		l.lexReadChar()
	}
	return l.input[position:l.position]
}

func isDigit(ch byte) bool {
	return '0' <= ch && ch <= '9'
}

// checks if the character is a letter or underscore
func isLetter(ch byte) bool {
	return 'a' <= ch && ch <= 'z' || 'A' <= ch && ch <= 'Z' || ch == '_'
}
