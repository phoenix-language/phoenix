import { parse } from "../deno/deps.ts";
import {
  display_discord_message,
  display_help_message,
  display_version_message,
} from "./Messages.ts";

const parsedArgs = parse(Deno.args);

const final_args = Object.keys(parsedArgs)[1]; // Gets the first argument

if (import.meta.url) {
  (function () {
    switch (final_args) {
      // Help CLi Command
      case "help":
        console.log(display_help_message());
        break;
      // Discord CLi Command
      case "discord":
        console.log(display_discord_message());
        break;
      // Version Command
      case "version":
        console.log(display_version_message());
        break;
      // Default argument handler
      default:
        console.log(display_help_message());
    }
  })();
}
