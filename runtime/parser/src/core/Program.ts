import { NodeTypes } from "../constants/NodeTypes.ts";
import { StatementListModule } from "./StatmentList.ts";
import { ASTNode } from "./Types.ts";

export class ProgramModule {
    private _StatementListModule: StatementListModule;

    public constructor(statementListModule: StatementListModule) {
        this._StatementListModule = statementListModule;
    }

    public consume_program(): ASTNode {
        return {
            type: NodeTypes.Program,
            body: this._StatementListModule.get_initial_statement_list(),
        };
    }
}
