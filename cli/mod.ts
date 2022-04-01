import { parse } from "./deps.ts";
import { display_discord_message, display_help_message } from "./messages.ts";

const parsedArgs = parse(Deno.args);

function Phoenix_CLI() {
    if (import.meta.url) {
        (function () {
            switch (Object.keys(parsedArgs)[1]) {
                // D
                case "help":
                    console.log(display_help_message());
                    break;
                case "discord": 
                    console.log(display_discord_message());
                    break;
                default:
                    console.log(display_help_message());
            }
        })();
    }
}

Phoenix_CLI();