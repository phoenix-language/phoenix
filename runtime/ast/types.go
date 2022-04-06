package ast

import (
	tokenizer "github.com/phoenix-language/phoenix/runtime/tokens"
)

// DeclareStatement type for the declare keyword
type DeclareStatement struct {
	Token tokenizer.Token
	Name  *Identifier
	Value Expression
}

type Identifier struct {
	Token tokenizer.Token // the token.IDENT token
	Value string
}
