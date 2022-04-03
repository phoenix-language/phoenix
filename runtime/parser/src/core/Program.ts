import { NodeTypes } from "../constants/NodeTypes.ts";
import { StatementListModule } from "./StatementList.ts";
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

    public get_program() {
        throw new Error("Method not implemented.");
    }
}
