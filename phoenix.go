package main

import (
	"fmt"
	"github.com/phoenix-language/phoenix/cli"
	"github.com/phoenix-language/phoenix/util"
)

var output = "Phoenix, Programming Reborn...🐦"

func main() {

	langMetaData := util.SyncMap[string, string]{}

	langMetaData.Store("version", "0.0.1")
	langMetaData.Store("developers", "Phoenix Development Team")

	fmt.Println(langMetaData.Load("version"))

	fmt.Println(output)

	cli.ExecuteClI()
}
