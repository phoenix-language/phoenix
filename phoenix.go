package main

import (
	"fmt"
	"github.com/phoenix-language/phoenix/cli"
	"github.com/phoenix-language/phoenix/util"
)

var output = "Phoenix, Programming Reborn...🐦"
var langMetaData = util.SyncMap[string, string]

func main() {

	langMetaData.Store("version", "0.0.1")
	langMetaData.Store("developers", "Phoenix Development Team")

	fmt.Println(output)

	cli.ExecuteClI()
}
