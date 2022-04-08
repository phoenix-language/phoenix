# Website Commands

pub_website:
	@echo "Publishing website to github pages..."
	@echo "This script will upload the static content from hugo to github pages..."
	@echo "-------------------------------------------------------"
	node ./scripts/pages.js

# Built Commands

windows_build_script_path = bin/phoenix-windows.exe
windows_build_script_args = -tags=windows -installsuffix=windows

linux_build_script_path := bin/phoenix-linux.so

build_phoenix_for_windows:
	@echo "Building phoenix-lang for windows..."
	@go build -o $(windows_build_script_path) $(windows_build_script_args)
	@echo "Build complete. Executable send to ./runtime/bin/phoenix-windows.exe"

build_phoenix_for_linux:
	@echo "Building phoenix-lang for linux..."
	@go build -o $(linux_build_script_path)
	@echo "Build complete. Shared object send to ./runtime/bin/phoenix-linux.so"

# TODO make this work...
test_build_phoenix_for_windows:
	@echo "Nothing yet..."
