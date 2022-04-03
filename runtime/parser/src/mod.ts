import { _ParserSpecs } from "./constants/TokenTypes.ts";
import { ParserModule } from "./core/mod.ts";
import { ProgramModule } from "./core/Program.ts";
import { StatementListModule } from "./core/StatementList.ts";
import { TokenExecutionModule } from "./core/Tokenizer.ts";
import { TokenizerModule } from "./tokenizer/mod.ts";

export class PhoenixParserModule {
    private static _TokenExecutionModule?: TokenExecutionModule;
    private static _TokenizerModule?: TokenizerModule;
    private static _ParserModule?: ParserModule;
    private static _ProgramModule?: ProgramModule;
    private static _StatementListModule?: StatementListModule;

    public static get_tokenizer(): TokenizerModule {
        if (!this._TokenizerModule) {
            this._TokenizerModule = new TokenizerModule(_ParserSpecs);
        }

        return this._TokenizerModule;
    }

    public static get_token_executor(): TokenExecutionModule {
        if (!this._TokenExecutionModule) {
            this._TokenExecutionModule = new TokenExecutionModule(
                this.get_tokenizer(),
            );
        }

        return this._TokenExecutionModule;
    }

    public static get_statement_list(): StatementListModule {
        if (!this._StatementListModule) {
            this._StatementListModule = new StatementListModule(
                this.get_token_executor(),
            );
        }

        return this._StatementListModule;
    }
}
