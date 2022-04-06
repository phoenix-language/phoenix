package cli

import (
	"fmt"
	"os"

	"github.com/phoenix-language/phoenix/util"
)

// Execute Run the cli package
func Execute() {

	// check if the user has provided the correct number of arguments
	if len(os.Args) != 2 {
		fmt.Printf("Usage: %s <command> or %s help\n", util.GetProgramSubName(), util.GetProgramSubName())
		os.Exit(1)
	}

	util.InitializeVersionControl()

	// check if the user has provided a valid command and option
	switch os.Args[1] {
	case "version":
		versionCommand()
	case "build":
		buildCommand()
	case "info":
		infoCommand()
	case "run":
		runCommand()
	case "help":
		helpCommand()
	default:
		fmt.Printf("Invalid command: %s | Displaying help command.\n", os.Args[1])
		helpCommand()
		os.Exit(1)
	}
}
