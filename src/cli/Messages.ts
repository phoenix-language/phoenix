import { Phoenix_Meta_Data } from "../../metadata.ts";

export function display_help_message(): string {
    return `
    > Phoenix CLI v${Phoenix_Meta_Data.cli_version}

    Available commands:

    --help : display this message
    --discord : Sends our discord server information
    `;
}

export function display_discord_message(): string {
    return `
    > Phoenix CLI v${Phoenix_Meta_Data.cli_version}

    You can find our official discord server here: https://discord.gg/U4FmBUHzEP
    `;
}
