import { StatementModule } from "./mod.ts";
import { ASTNode } from "../Types.ts";
import { TokenTypes } from "../../constants/TokenTypes.ts";
import { ExpressionModule } from "../expressions/mod.ts";
import { NodeTypes } from "../../constants/NodeTypes.ts";

export class ConsoleStatement extends StatementModule {
    public getStatement(): ASTNode {
        this._TokenExecutionModule.consume_token_and_next(
            TokenTypes.CONSOLE_TYPE,
        );

        const expr = this._get_expression_list()

        this._TokenExecutionModule.consume_token_and_next(
            TokenTypes.SEMI_COLON_TYPE,
        );

        return {
            type: NodeTypes.ConsoleStatement,
            expressions: expr,
        };
    }

    private _get_expression_list() {
        const expressions: any[] = [];

        do {
            expressions.push(ConsoleStatement._get_expression());
        } while (
            this._TokenExecutionModule.get_active_token.type ===
                TokenTypes.COMMA_TYPE &&
            this._TokenExecutionModule.consume_token_and_next(
                TokenTypes.COMMA_TYPE,
            )
        );

        return expressions;
    }

    private static _get_expression() {
        return ExpressionModule.getExpressionImpl(
            NodeTypes.AssignmentExpression,
        ).getExpression();
    }
}
