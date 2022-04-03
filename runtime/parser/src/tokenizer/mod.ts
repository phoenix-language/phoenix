import { Token, Tokenizer } from "./types.ts";
import type { ParserSpecs } from "../constants/TokenTypes.ts";
import { IllegalStateTypeException } from "../../../exceptions/Exception.ts";

export class TokenizerModule implements Tokenizer {
    private _specs: ParserSpecs;
    private _string_to_tokenize: string | undefined = undefined;
    private _current_token_index: number;

    public constructor(specs: ParserSpecs) {
        this._specs = specs;
        this._current_token_index = 0;
    }

    public init(string_to_tokenize: string): void {
        this._string_to_tokenize = string_to_tokenize;
        this._current_token_index = 0;
    }

    public isEOF(): boolean {
        if (!this._string_to_tokenize) return true;

        return this._current_token_index === this._string_to_tokenize.length;
    }

    public has_more_tokens(): boolean {
        if (!this._string_to_tokenize) return false;

        return this._current_token_index < this._string_to_tokenize.length;
    }

    public get_next_token(): Token | null {
        if (!this._string_to_tokenize) {
            throw new IllegalStateTypeException(
                `TokenizerModule.get_next_token() called before TokenizerModule.init()`,
            );
        }

        if (this.has_more_tokens()) {
            return null;
        }

        const token_string = this._string_to_tokenize.slice(
            this._current_token_index,
        );

        for (const { regex, tokenType } of this._specs) {
            const tokenValue = this.if_token_matched(regex, token_string);

            if (tokenValue === null) {
                continue;
            }

            if (tokenType === null) {
                return this.get_next_token();
            }

            return {
                type: tokenType,
                value: tokenValue,
            };
        }

        throw new SyntaxError(
            `We ran into an error!!...Unexpected token: "${token_string[0]}"`,
        );
    }

    private if_token_matched(regex: RegExp, string: string): string | null {
        const matched = regex.exec(string);
        if (matched === null) {
            return null;
        }
        this._current_token_index += matched[0].length;
        return matched[0];
    }
}
