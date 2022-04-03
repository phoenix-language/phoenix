import { TokenTypes } from "../constants/TokenTypes.ts";
import { TokenExecutionModule } from "./Tokenizer.ts";

export class StatementListModule {
  private _tokenExecutionModule: TokenExecutionModule;

  public constructor(tokenExecutionModule: TokenExecutionModule) {
    this._tokenExecutionModule = tokenExecutionModule;
  }

  public get_initial_statement_list() {
    // TODO: Implement
  }
}
