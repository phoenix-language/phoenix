build_phoenix_for_windows:
	@echo "Building phoenix-lang for windows..."
#	@go build -o bin/phoenix-windows-v0-0-1.exe -ldflags "-s -w" github.com/phoenix-language/phoenix \
#		-buildmode=c-shared -tags=windows -installsuffix=windows
	@go build -o bin/phoenix-windows.exe -tags=windows -installsuffix=windows
	@echo "Build complete. Executable is in bin/phoenix-windows.exe"

build_phoenix_for_linux:
	@echo "Building phoenix-lang for linux..."
	@go build -o bin/phoenix-linux.so
	@echo "Build complete. Shared object is in bin/phoenix-linux.so"
