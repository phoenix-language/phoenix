package main

import (
	"fmt"

	"github.com/phoenix-language/phoenix/cli"
)

var output = "Phoenix, Programming Reborn...🐦"

func main() {
	fmt.Println(output)

	cli.ExecuteClI()
}
