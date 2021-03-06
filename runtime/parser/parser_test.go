package parser

import (
	"github.com/phoenix-language/phoenix/runtime/ast"
	"github.com/phoenix-language/phoenix/runtime/lexer"
	"testing"
)

// TODO get working...
func TestDeclareStatements(t *testing.T) {
	input := `
		declare a :: 1;
		declare b :: 2;
		declare c :: 3;
	`

	l := lexer.LexNewChar(input)
	p := Create(l)

	program := p.ParseProgram()
	checkParserErrors(t, p)

	if program == nil {
		t.Fatalf("ParseProgram() returned nil")
	}

	if len(program.Statements) != 3 {
		t.Fatalf("program.Statements does not contain 3 statements. got=%d", len(program.Statements))
	}

	tests := []struct {
		expectedIdentifier string
	}{
		{"a"},
		{"b"},
		{"c"},
	}

	for i, tt := range tests {
		stmt := program.Statements[i]
		if !testDeclareStatement(t, stmt, tt.expectedIdentifier) {
			return
		}
	}
}

func testDeclareStatement(t *testing.T, stmt ast.Statement, identifier string) bool {
	if stmt.TokenLiteral() != "declare" {
		t.Errorf("stmt.TokenLiteral not 'declare'. got=%q", stmt.TokenLiteral())
		return false
	}

	declareStmt, ok := stmt.(*ast.DeclareStatement)

	if !ok {
		t.Errorf("stmt not *ast.DeclareStatement. got=%T", stmt)
		return false
	}

	if declareStmt.Name.Value != identifier {
		t.Errorf("declareStmt.Identifier.Value not '%s'. got=%s", identifier, declareStmt.Name.Value)
		return false
	}

	if declareStmt.Name.TokenLiteral() != identifier {
		t.Errorf("declareStmt.Identifier.TokenLiteral() not '%s'. got=%s", identifier, declareStmt.Name.TokenLiteral())
		return false
	}

	return true
}

// helper func for checking if an error has occurred during testing of the parser.
// if an error has occurred, the test will fail.
func checkParserErrors(t *testing.T, p *Parser) {
	errors := p.Errors()

	if len(errors) == 0 {
		return
	}

	t.Errorf("parser has %d errors", len(errors))
	for _, msg := range errors {
		t.Errorf("parser error: %q", msg)
	}
	t.FailNow()
}
