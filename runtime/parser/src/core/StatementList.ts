import { TokenTypes } from "../constants/TokenTypes.ts";
import { TokenExecutionModule } from "./Tokenizer.ts";
import {StatementModule} from "./statements/mod.ts";
import {PhoenixParserModule} from "../mod.ts";

export class StatementListModule {
    private _tokenExecutionModule: TokenExecutionModule;

    public constructor(tokenExecutionModule: TokenExecutionModule) {
        this._tokenExecutionModule = tokenExecutionModule;
    }

    public get_initial_statement_list() {
        for(
            let _token = this._tokenExecutionModule.get_active_token;
            _token !== null && _token.type !== TokenTypes.CREATE_NEW_PROGRAM_TYPE;
            _token = this._tokenExecutionModule.get_active_token
        ) {
            this._tokenExecutionModule.consume_token_and_next(_token.type)
        }

        return PhoenixParserModule.get_statement_list()
    }

    public get_statement_list(food_token: string) {
       const statements: Array<any> = [];

       for(
           let _token = this._tokenExecutionModule.get_active_token;
           _token !== null && _token.type !== food_token;
           _token = this._tokenExecutionModule.get_active_token
       ) {
           statements.push(StatementModule.getStatementImpl(_token).getStatement());
       }

       return statements;
    }
}
