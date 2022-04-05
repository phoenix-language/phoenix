package exception

import (
	"fmt"
)

type Error struct {
	// The line the error hit on
	line int
	// The message for the error
	message string
}

// Levels
//
// 0 - Warning
// 1 - Error
// 2 - Fatal
func CreateGeneralException(name string, message string, level int) {
	if level == 0 {
		fmt.Printf("[WARN] - %s: %s\n", name, message)
	} else if level == 1 {
		fmt.Printf("[ERROR] - %s: %s\n", name, message)
	} else if level == 2 {
		fmt.Printf("[FATAL] - %s: %s\n", name, message)
	}
}
