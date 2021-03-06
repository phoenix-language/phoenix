package util

import (
	"fmt"
	"time"
)

const (
	// ProgramName The name of the program
	ProgramName = "The Phoenix Programming Language"
)

// Build script injections

var (
	VersionState = "Alpha"
	Version      = "0.0.0"
	BuildDate    = "unknown-build-date"
	// sub name for cli commands
	programSubName string
)

var (
	// ProgramFullName The full name of the program
	ProgramFullName              string
	ProgramFullNameWithBuildDate string
	CreatedAt                    time.Time
	GitHubRepoUrl                = "https://github.com/phoenix-language/phoenix"
	DocumentationUrl             = "n/a"
)

// GetProgramSubName Returns the current subname of the program
func GetProgramSubName() string {
	return programSubName
}

// SetProgramSubName Sets a new subname for the program
func SetProgramSubName(name string) {
	programSubName = name
}

func UpdateProgramFullNames() {
	if programSubName != "" {
		ProgramFullName = fmt.Sprintf("%s %s %s (%s)", ProgramName, programSubName, Version, VersionState)
		ProgramFullNameWithBuildDate = fmt.Sprintf("%s %s %s (%s)", ProgramName, programSubName, Version, BuildDate)
		return
	}

	ProgramFullName = fmt.Sprintf("%s %s (%s)", ProgramName, Version, VersionState)
	ProgramFullNameWithBuildDate = fmt.Sprintf("%s %s %s", ProgramName, Version, BuildDate)

}

// InitializeVersionControl Starts the version control program
func InitializeVersionControl() {
	CreatedAt = time.Now().Local()
	SetProgramSubName("phoenix")
	UpdateProgramFullNames()
}
