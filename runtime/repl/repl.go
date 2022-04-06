package repl

import (
	"bufio"
	"fmt"
	exception "github.com/phoenix-language/phoenix/runtime/exceptions"
	"github.com/phoenix-language/phoenix/runtime/lexer"
	tokenizer "github.com/phoenix-language/phoenix/runtime/tokens"
	"io"
)

const PromptText = ">> "

// Create generates a new console read output for interactive access to the lexer
func Create(in io.Reader, out io.Writer) {
	scanner := bufio.NewReader(in)

	for {
		fmt.Printf(PromptText)

		line, err := scanner.ReadString('\n')
		if err != nil {
			exception.CreateGeneralException("repl create func", err, 1)
		}

		l := lexer.LexNewChar(line)

		for tok := l.LexNextToken(); tok.Type != tokenizer.EOF; tok = l.LexNextToken() {
			fmt.Printf("%+v\n", tok)
		}
	}
}
