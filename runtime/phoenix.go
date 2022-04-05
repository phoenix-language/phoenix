package main

import (
	"fmt"

	tokenizer "github.com/phoenix-language/phoenix/runtime/token"
)

func main() {
	fmt.Println("Phoenix, Programming Reborn...🐦")

	// loop through all the tokens and print them out
	for _, token := range make(tokenizer.TokenList, 100) {
		fmt.Println("=== List of Tokens ===")
		fmt.Println(token)
	}
}
