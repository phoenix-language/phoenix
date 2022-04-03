import { NodeTypes } from "../../constants/NodeTypes.ts";

export class ExpressionModule {
    static getExpressionImpl(
        expressionType: keyof typeof NodeTypes,
    ): ExpressionModule {
        return this;
    }
}
