import {Exception} from "../exceptions/mod.ts";

export class Parser_Result {

    private error: Exception | null
    private last_registered_advance_count: number;
    private node: null;
    private advance_count: number;
    private to_reverse_count: number;

    public constructor() {
        this.error = null;
        this.node = null;
        this.last_registered_advance_count = 0;
        this.advance_count = 0;
        this.to_reverse_count = 0;
    }

    private generateAdvancement(): void {
        this.last_registered_advance_count = 1
        this.advance_count++;
    }

    /**
     * @param result
     * @private
     */
    private generate(result: this) {
        this.last_registered_advance_count = result.last_registered_advance_count;
        this.advance_count += result.advance_count;
        if(result.error) {
            this.error = result.error;
        }
        return result.node;
    }

    /**
     * @param error
     * @private
     */
    private failure(error: Exception): this {
        if(!this.error || this.advance_count == 0) {
            this.error = error;
        }
        return this
    }
}