package cli

import (
	"bufio"
	"os"
)

func Run() {
	terminalReader := bufio.NewReader(os.Stdin)

	for {
		input, err := terminalReader.ReadString('\n')

		if err != nil {
			panic(err)
		}

		println("You typed: ", input)
		println("press 'CTRL-C' to exit the program.")
	}
}
