import { TokenTypes } from "../constants/TokenTypes.ts";
import { Token, Tokenizer } from "../tokenizer/types.ts";

export class TokenExecutionModule {
    private _tokenizer: Tokenizer;
    private _active_token: Token | null = null;

    public constructor(tokenizer: Tokenizer) {
        this._tokenizer = tokenizer;
    }

    /**
     * Processes the next token in the tokenizer
     * @param tokenType The type of token to process
     * @returns The token that was processed or null if there was no token to process
     */
    public consume_token_and_next(tokenType: string | null): Token | null {
        const _token = this._active_token;

        if (_token == null) {
            throw new SyntaxError(
                `Unexpected end of input, expected : "${tokenType}"`,
            );
        }

        if (_token.type !== tokenType) {
            throw new SyntaxError(
                `Unexpected token "${_token.type}", expected : "${tokenType}"`,
            );
        }

        this.set_next_consume_token(this._tokenizer.get_next_token());

        return _token;
    }

    public consume_semicolon_tokens(): void {
        if (this.get_active_token?.type == TokenTypes.SEMI_COLON_TYPE) {
            this.consume_token_and_next(TokenTypes.SEMI_COLON_TYPE);
        }
    }

    /**
     * Sets the next active token after its consumed by the tokenizer
     * @param food The token to consume if it exists
     */
    public set_next_consume_token(food: Token | null): void {
        this._active_token = food;
    }

    public get get_active_token(): Token | null {
        return this._active_token;
    }
}
