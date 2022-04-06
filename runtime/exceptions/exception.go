package exception

import (
	"fmt"
	"os"
)

type Error struct {
	// The line the error hit on
	line int
	// The message for the error
	message string
}

// CreateGeneralException Levels
//
// 0 - Warning (Do not quit the program)
//
// 1 - Error (They do quit the program)
//
// 2 - Fatal (They do quit the program)
func CreateGeneralException(name string, message error, level int) {
	if level == 0 {
		fmt.Printf("[WARN] - %s: %s\n", name, message)
	} else if level == 1 {
		fmt.Printf("[ERROR] - %s: %s\n", name, message)
		os.Exit(1)
	} else if level == 2 {
		fmt.Printf("[FATAL] - %s: %s\n", name, message)
		os.Exit(1)
	} else {
		panic("[INTERNAL-RUNTIME-ERROR] - Unknown Exception Level. Options: 0, 1, 2")
	}
}
