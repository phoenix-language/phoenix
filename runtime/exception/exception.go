package runtime_exception

import (
	"fmt"
	"io"
	"os"
)

// Handle Exceptions

var W io.Writer = os.Stdout

type Error struct {
	// The line the error hit on
	line int
	// The message for the error
	message string
}

func (e *Error) PrintErrorException() {
	fmt.Printf("Error (line %d): %s", e.line, e.message)
	os.Exit(1)
}

// Prints a new error to the console
func GenerateNewException(line int, message string) *Error {
	err := &Error{
		line:    line,
		message: message,
	}

	fmt.Fprintln(W, err)

	return err
}
