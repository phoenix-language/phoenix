import Token from "../lexer/Token.ts";

export class Parser {
    private readonly tokens: Token[];
    private token_index: number;
    private current_token: Token;
    
    public constructor(_tokens: Token[]) {
        this.tokens = _tokens;
        this.token_index = 0;
        this.current_token = this.tokens[this.token_index];
        this.advance()
    }

    private advance(): Token {
        this.token_index++;
        this.updateTokenQueue();
        return this.tokens[this.token_index - 1];
    }

    private updateTokenQueue(): void {
        if(this.token_index >= 0 && this.token_index < this.tokens.length) {
            this.current_token = this.tokens[this.token_index];
        }
    }

}