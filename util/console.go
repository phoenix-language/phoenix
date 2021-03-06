package util

import (
	"os"
	"os/exec"
	"runtime"
)

var clear map[string]func() //create a map for storing clear functions

func init() {

	clear = make(map[string]func()) //Initialize it

	clear["linux"] = func() {
		cmd := exec.Command("clear") //Linux example, its tested
		cmd.Stdout = os.Stdout
		err := cmd.Run()

		if err != nil {
			panic(err)
		}
	}
	clear["windows"] = func() {
		cmd := exec.Command("cmd", "/c", "cls", "clear") //Windows example, its tested
		cmd.Stdout = os.Stdout
		err := cmd.Run()

		if err != nil {
			panic(err)
		}
	}
}

// ConsoleClear Function to wipe previous the terminal screen
func ConsoleClear() {
	//runtime.GOOS -> linux, windows, darwin etc.
	value, ok := clear[runtime.GOOS]
	//if we defined a clear func for that platform:
	if ok {
		//we execute it
		value()
	} else {
		//unsupported platform
		panic("Your platform is unsupported! I can't clear terminal screen :(")
	}
}
