build_phoenix_for_windows:
	@echo "Building phoenix-lang for windows..."
#	@go build -o bin/phoenix-windows-v0-0-1.exe -ldflags "-s -w" github.com/phoenix-language/phoenix \
#		-buildmode=c-shared -tags=windows -installsuffix=windows
	@go build -o bin/phoenix-windows.exe -tags=windows -installsuffix=windows
	@echo "Build complete. Executable send to bin/phoenix-windows.exe"

build_phoenix_for_linux:
	@echo "Building phoenix-lang for linux..."
	@go build -o bin/phoenix-linux.so
	@echo "Build complete. Shared object send to bin/phoenix-linux.so"

# TODO make this work...
test_build_phoenix_for_windows:
	# Go into the bin dir
	@cd ./bin
	# exc the language cli run command
	@./phoenix-windows.exe run
	# input some fake code to the lexer
	@echo declare x :: 5 + 5
	@cd ..
	@echo "Windows lexer test complete!"