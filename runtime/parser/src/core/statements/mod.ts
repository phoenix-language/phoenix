import { TokenTypes } from "../../constants/TokenTypes.ts";
import { PhoenixParserModule } from "../../mod.ts";
import { Token } from "../../tokenizer/types.ts";
import { TokenExecutionModule } from "../Tokenizer.ts";
import { ASTNode } from "../Types.ts";

export abstract class StatementModule {
    protected _TokenExecutionModule: TokenExecutionModule;

    public constructor(TokenExecutionModule: TokenExecutionModule) {
        this._TokenExecutionModule = TokenExecutionModule;
    }

    public abstract getStatement(): ASTNode;

    static getStatementImpl(consumption: Token): StatementModule {
        switch (consumption.type) {
            case TokenTypes.CONSOLE_TYPE:
                return PhoenixParserModule.getPrintStatement();

            case TokenTypes.SEMI_COLON_TYPE:
                return PhoenixParserModule.getEmptyStatement();

            case TokenTypes.OPEN_CURLY_BRACE_TYPE:
                return PhoenixParserModule.getBlockStatement();

            case TokenTypes.ASSIGN_TYPE:
                return PhoenixParserModule.getVariableStatement();

            case TokenTypes.IF_TYPE:
                return PhoenixParserModule.getIfStatement();

            case TokenTypes.WHILE_TYPE:
                return PhoenixParserModule.getWhileStatement();

            case TokenTypes.END_TYPE:
                return PhoenixParserModule.getBreakStatement();

            case TokenTypes.PASS_TYPE:
                return PhoenixParserModule.getContinueStatement();

            default:
                return PhoenixParserModule.getExpressionStatement();
        }
    }
}
