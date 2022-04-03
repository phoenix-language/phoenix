import { TokenizerModule } from "../tokenizer/mod.ts";
import { ProgramModule } from "./Program.ts";
import { TokenExecutionModule } from "./Tokenizer.ts";

export class ParserModule {
    private _TokenExecutionModule: TokenExecutionModule;
    private _ProgramModule: ProgramModule;
    private _TokenizerModule: TokenizerModule;
    private _string_to_token: string;

    /**
     * @param tokenExecutionModule
     * @param programModule
     * @param tokenizerModule
     */
    public constructor(
        tokenExecutionModule: TokenExecutionModule,
        programModule: ProgramModule,
        tokenizerModule: TokenizerModule,
    ) {
        this._TokenExecutionModule = tokenExecutionModule;
        this._ProgramModule = programModule;
        this._TokenizerModule = tokenizerModule;
        this._string_to_token = "";
    }

    /**
     * @param string_to_tokenize The string to tokenize
     * @returns {void}
     */
    public parse(string_to_tokenize: string): void {
        this._string_to_token = string_to_tokenize;

        this._TokenizerModule.init(this._string_to_token);

        // initialize consumption of tokens
        this._TokenExecutionModule.set_next_consume_token(
            this._TokenizerModule.get_next_token(),
        );

        return this._ProgramModule.get_program();
    }
}
