import { Lexer } from "../src/lexer/Lexer.ts";

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

execute("mock.phx", "10 + 20");
