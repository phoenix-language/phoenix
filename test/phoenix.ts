import { Lexer } from "../runtime/lexer/Lexer.ts";

console.log("Programming reborn...🐦 Welcome to Phoenix!");

/**
 * @param file_name The name of the file we are tokenizing.
 * @param file_content Our source code to execute.
 * @returns
 */
function execute(file_name: string, file_content: string): void {
    console.log(`Executing ${file_name}...`);

    if (file_content.trim() === "") {
        console.log("File is empty!");
        return;
    }

    try {
        const _lexer = new Lexer(file_name, file_content);
        const { error, tokens } = _lexer.generateTokens();
        if (error) {
            return error.createException();
        }

        const token_cache: any = [];

        tokens.forEach((token: { toString: () => any }) => {
            token_cache.push(token.toString());
        });

        console.log(token_cache);
    } catch (e) {
        console.log(e);
    }
}

execute("mock.phx", "1 - 200 * 2 (48.3 * 3 (50 * 7) - 5) + 3");

// Lexers to:

//     [
//     "INT:1",      "MINUS",
//         "INT:200",    "MULTIPLY",
//         "INT:2",      "LPAREN",
//         "FLOAT:48.3", "MULTIPLY",
//         "INT:3",      "LPAREN",
//         "INT:50",     "MULTIPLY",
//         "INT:7",      "RPAREN",
//         "MINUS",      "INT:5",
//         "RPAREN",     "PLUS:+",
//         "INT:3",      "EOF"
//     ]
