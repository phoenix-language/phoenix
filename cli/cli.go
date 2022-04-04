package main

import (
	"fmt"
	"os"

	"github.com/phoenix-language/phoenix/util"
)

func main() {

	// check if the user has provided the correct number of arguments
	if len(os.Args) != 2 {
		fmt.Println("Usage: phoenix <command> or get help using phoenix help")
		os.Exit(1)
	}

	util.InitializeVersionControl()

	// check if the user has provided a valid command and option
	switch os.Args[1] {
	case "version":
		versionCommand()
	case "build":
		buildCommand()
	case "run":
		runCommand()
	case "help":
		helpCommand()
	default:
		fmt.Printf("Invalid command: %s | Use phoenix help to get a list of commands", os.Args[1])
		fmt.Print("\n")
		os.Exit(1)
	}
}

// Shows the help information
func helpCommand() {
	fmt.Println("Usage: phoenix <command> [option]")
	fmt.Println("Commands:")
	fmt.Println("  version")
	fmt.Println("  help")
	fmt.Println("  build <file>.phx")
	fmt.Println("  run <file>.phx")
}

// Shows the version information
func versionCommand() {
	fmt.Printf("%s %s %s\n", util.ProgramName, util.VersionState, util.Version)
}

func infoCommand() {
	fmt.Printf(" === %s ===\n", util.ProgramName)
	fmt.Printf("Version: %s\n", util.Version)
	fmt.Printf("Build Date: %s\n", util.BuildDate)
	fmt.Printf("Created At: %s\n", util.CreatedAt)

}

// Compiles the phoenix language file given
func buildCommand() {
	fmt.Println("Build command ran! *fake compilation*")
}

// Runs the language runner
func runCommand() {
	fmt.Println("Ran command ran! *fake execution*")
}
