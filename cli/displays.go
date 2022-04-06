package cli

import (
	"fmt"
	"os"
	"os/user"

	"github.com/phoenix-language/phoenix/runtime/repl"
	"github.com/phoenix-language/phoenix/util"
)

// Shows the help information
func helpCommand() {
	fmt.Println("Usage: phoenix <command> [option]")
	fmt.Println("Commands:")
	fmt.Println("  version - Shows the version information")
	fmt.Println("  help   - Shows the help information")
	fmt.Println("  build <file>.phx - Compiles your phoenix program file")
	fmt.Println("  run <file>.phx - Runs your phoenix program file")
	fmt.Println("  info - Shows the information about the phoenix language")
}

// Shows the version information
func versionCommand() {
	fmt.Printf("%s %s %s\n", util.ProgramName, util.VersionState, util.Version)
}

func infoCommand() {
	fmt.Printf(" === %s ===\n", util.ProgramName)
	fmt.Printf("Version: %s | Stability: %s\n", util.Version, util.VersionState)
	fmt.Printf("Build Date: %s\n", util.BuildDate)
	fmt.Printf("Created At: %s\n", util.CreatedAt)

}

// Compiles the phoenix language file given
func buildCommand() {
	fmt.Println("Build command ran! *fake compilation*")
}

// Runs the language runner
func runCommand() {
	cmdUser, err := user.Current()

	if err != nil {
		panic(err)
	}

	util.ConsoleClear()

	fmt.Printf("@%s | Phoenix REPL Tool\n", cmdUser.Username)
	fmt.Println("Lexer testing, type some valid phoenix below...")

	repl.Create(os.Stdin, os.Stdout)
}
