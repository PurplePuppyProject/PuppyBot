//! The slash commands of PuppyBot on Discord, and its answers to them. The texts are typed texts
//! of `puppybot-core`, in German and English.

use std::collections::BTreeSet;

use puppybot_core::i18n::Lang;
use puppybot_core::text::Text;
use twilight_model::application::command::{Command, CommandType};
use twilight_model::application::interaction::{
    Interaction, InteractionContextType, InteractionData, InteractionType,
};
use twilight_model::channel::message::{AllowedMentions, MessageFlags};
use twilight_model::http::interaction::InteractionResponse;
use twilight_model::id::Id;
use twilight_model::id::marker::GuildMarker;
use twilight_util::builder::command::CommandBuilder;
use twilight_util::builder::interaction_response::ChannelMessageBuilder;

/// Discord's locale of the German texts. The English ones are the defaults.
const GERMAN: &str = "de";

/// The slash commands of PuppyBot, for each of its servers.
#[must_use]
pub fn commands() -> Vec<Command> {
    vec![
        CommandBuilder::new(
            "info",
            Text::InfoDescription.render(Lang::En),
            CommandType::ChatInput,
        )
        .description_localizations([(GERMAN, Text::InfoDescription.render(Lang::De))])
        .contexts([InteractionContextType::Guild])
        .build(),
    ]
}

/// The answer to `interaction` if it is a slash command, on one of `guilds` or not: only the
/// person who asked sees it, in their language, and it mentions nobody.
#[must_use]
pub fn answer(
    interaction: &Interaction,
    guilds: &BTreeSet<Id<GuildMarker>>,
) -> Option<InteractionResponse> {
    let Some(InteractionData::ApplicationCommand(command)) = &interaction.data else {
        return None;
    };
    if interaction.kind != InteractionType::ApplicationCommand {
        return None;
    }
    let text = if !interaction
        .guild_id
        .is_some_and(|guild| guilds.contains(&guild))
    {
        Text::NotHere
    } else if command.name == "info" {
        Text::Info {
            version: crate::VERSION,
        }
    } else {
        Text::UnknownCommand
    };
    let lang = Lang::from_locale(interaction.locale.as_deref().unwrap_or_default());
    Some(
        ChannelMessageBuilder::new()
            .content(text.render(lang))
            .flags(MessageFlags::EPHEMERAL)
            .allowed_mentions(AllowedMentions::default())
            .build(),
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use twilight_model::http::interaction::InteractionResponseType;

    use super::*;

    /// The server of the tests.
    pub(crate) const GUILD: u64 = 1_100_000_000_000_000_001;

    /// An interaction as Discord sends it: the slash command `name` of the type `kind`, by
    /// someone with the locale `locale`, on the server `guild` (none for a direct message).
    pub(crate) fn interaction(
        kind: u8,
        name: &str,
        locale: &str,
        guild: Option<u64>,
    ) -> Interaction {
        let guild = guild.map_or_else(String::new, |guild| format!(r#""guild_id": "{guild}","#));
        serde_json::from_str(&format!(
            r#"{{
                "id": "1200000000000000001",
                "application_id": "1200000000000000002",
                "type": {kind},
                "token": "interaction-token",
                "version": 1,
                "locale": "{locale}",
                "attachment_size_limit": 26214400,
                "authorizing_integration_owners": {{}},
                "entitlements": [],
                {guild}
                "data": {{"id": "1200000000000000003", "name": "{name}", "type": 1}}
            }}"#
        ))
        .unwrap()
    }

    fn guilds() -> BTreeSet<Id<GuildMarker>> {
        BTreeSet::from([Id::new(GUILD)])
    }

    /// The text of `response`, and whether it is ephemeral and mentions nobody.
    fn content(response: &InteractionResponse) -> &str {
        assert_eq!(
            response.kind,
            InteractionResponseType::ChannelMessageWithSource
        );
        let data = response.data.as_ref().unwrap();
        assert_eq!(data.flags, Some(MessageFlags::EPHEMERAL));
        assert_eq!(data.allowed_mentions, Some(AllowedMentions::default()));
        data.content.as_deref().unwrap()
    }

    #[test]
    fn info_in_the_language_of_the_person() {
        let version = crate::VERSION;
        for (locale, text) in [
            ("de", format!("PuppyBot {version}, ein Discord-Bot")),
            ("en-US", format!("PuppyBot {version}, a Discord bot")),
            ("fr", format!("PuppyBot {version}, a Discord bot")),
        ] {
            let interaction = interaction(2, "info", locale, Some(GUILD));
            let response = answer(&interaction, &guilds()).unwrap();
            assert!(content(&response).starts_with(&text), "{locale}");
        }
    }

    #[test]
    fn commands_it_does_not_know() {
        let interaction = interaction(2, "dance", "en-GB", Some(GUILD));
        let response = answer(&interaction, &guilds()).unwrap();
        assert_eq!(
            content(&response),
            "PuppyBot doesn't know this command (any more)."
        );
    }

    #[test]
    fn commands_outside_its_servers() {
        for guild in [Some(GUILD + 1), None] {
            let interaction = interaction(2, "info", "de", guild);
            let response = answer(&interaction, &guilds()).unwrap();
            assert_eq!(
                content(&response),
                "PuppyBot arbeitet nur auf seinen Servern."
            );
        }
    }

    #[test]
    fn no_answer_but_to_slash_commands() {
        // Autocomplete has the data of a command, but isn't one.
        let autocomplete = interaction(4, "info", "de", Some(GUILD));
        assert_eq!(answer(&autocomplete, &guilds()), None);
        let mut without_data = interaction(2, "info", "de", Some(GUILD));
        without_data.data = None;
        assert_eq!(answer(&without_data, &guilds()), None);
    }

    #[test]
    fn the_commands_in_both_languages() {
        let commands = commands();
        let names: Vec<&str> = commands
            .iter()
            .map(|command| command.name.as_str())
            .collect();
        assert_eq!(names, ["info"]);
        let info = &commands[0];
        assert_eq!(
            info.description,
            "Shows what PuppyBot is and which version runs"
        );
        assert_eq!(
            info.description_localizations.as_ref().unwrap()["de"],
            "Zeigt, was PuppyBot ist und welche Version läuft"
        );
        assert_eq!(info.contexts, Some(vec![InteractionContextType::Guild]));
    }
}
