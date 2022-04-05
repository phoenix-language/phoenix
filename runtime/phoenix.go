package main

import (
	"fmt"

	tokenizer "github.com/phoenix-language/phoenix/runtime/token"
)

func main() {
	fmt.Println("Phoenix, Programming Reborn...🐦")

	// loop through all the keywords and print them out
	for _, keyword := range tokenizer.Keywords {
		fmt.Printf("Keyword value: %d \n", keyword)
	}
}
