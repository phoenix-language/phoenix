import { Phoenix_Meta_Data } from "../meta/metadata.ts";

export function display_help_message(): string {
  return `
    > The Phoenix Programming Language | https://github.com/phoenix-language/phoenix

    Available commands:

    --help
        shows this message.
    --discord
        Sends our discord server information.
    --version
        Shows the current version of the cli and runtime.
    `;
}

export function display_discord_message(): string {
  return `
    You can find our official discord server here: https://discord.gg/U4FmBUHzEP
    `;
}

export function display_invalid_argument(arg: string): string {
  return `
        Argument '${arg}' was not expected, or isn't a valid option.
        
        USAGE:
            phoenix [Argument]
            
        For more information try --help
    `;
}

export function display_version_message(): string {
  return `
    Phoenix CLI v${Phoenix_Meta_Data.cli_version}
    Phoenix Runtime v${Phoenix_Meta_Data.runtime_version}
    
    Use the --upgrade command to get the latest version of Phoenix.  
    `;
}
