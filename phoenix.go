package main

import (
	"fmt"
	"log"
	"os"
)

var output = "Phoenix, Programming Reborn...🐦"

func main() {
	fmt.Println(output)

	file, err := os.Open("README.md") // For read access.
	if err != nil {
		log.Fatal(err)
	}

	data := make([]byte, 10000)
	count, err := file.Read(data)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("read %d bytes: %q\n", count, data[:count])
}
