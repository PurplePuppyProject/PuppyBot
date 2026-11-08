//! The slash commands of PuppyBot on Discord: their definitions, what an interaction asks for,
//! and the answers. The texts are typed texts of `puppybot-core`, in German and English.

use std::collections::BTreeSet;

use puppybot_core::dice::{self, Dice};
use puppybot_core::i18n::Lang;
use puppybot_core::text::Text;
use puppybot_core::text_command;
use twilight_model::application::command::{Command, CommandType};
use twilight_model::application::interaction::application_command::{
    CommandData, CommandDataOption, CommandOptionValue,
};
use twilight_model::application::interaction::{
    Interaction, InteractionContextType, InteractionData, InteractionType,
};
use twilight_model::channel::message::{AllowedMentions, MessageFlags};
use twilight_model::guild::Permissions;
use twilight_model::http::interaction::InteractionResponse;
use twilight_model::id::Id;
use twilight_model::id::marker::GuildMarker;
use twilight_util::builder::command::{
    BooleanBuilder, CommandBuilder, StringBuilder, SubCommandBuilder,
};
use twilight_util::builder::interaction_response::ChannelMessageBuilder;

/// Discord's locale of the German texts. The English ones are the defaults.
const GERMAN: &str = "de";

/// Who may use /settings: people who may manage the server. Discord shows the command only to
/// them, unless the server's admins allow it to others, and PuppyBot checks it again.
const MAY_CHANGE_SETTINGS: Permissions = Permissions::MANAGE_GUILD;

/// The slash commands of PuppyBot, for each of its servers.
#[must_use]
pub fn commands() -> Vec<Command> {
    let to = StringBuilder::new("to", Text::LanguageToDescription.render(Lang::En))
        .description_localizations([(GERMAN, Text::LanguageToDescription.render(Lang::De))]);
    let language = SubCommandBuilder::new("language", Text::LanguageDescription.render(Lang::En))
        .description_localizations([(GERMAN, Text::LanguageDescription.render(Lang::De))])
        .option(languages(to));
    let to = StringBuilder::new("to", Text::PrefixToDescription.render(Lang::En))
        .description_localizations([(GERMAN, Text::PrefixToDescription.render(Lang::De))])
        .min_length(1)
        .max_length(u16::from(text_command::MAX_PREFIX));
    let prefix = SubCommandBuilder::new("prefix", Text::PrefixDescription.render(Lang::En))
        .description_localizations([(GERMAN, Text::PrefixDescription.render(Lang::De))])
        .option(to);
    vec![
        CommandBuilder::new(
            "info",
            Text::InfoDescription.render(Lang::En),
            CommandType::ChatInput,
        )
        .description_localizations([(GERMAN, Text::InfoDescription.render(Lang::De))])
        .contexts([InteractionContextType::Guild])
        .build(),
        CommandBuilder::new(
            "settings",
            Text::SettingsDescription.render(Lang::En),
            CommandType::ChatInput,
        )
        .description_localizations([(GERMAN, Text::SettingsDescription.render(Lang::De))])
        .contexts([InteractionContextType::Guild])
        .default_member_permissions(MAY_CHANGE_SETTINGS)
        .option(language)
        .option(prefix)
        .build(),
        dice(),
    ]
}

/// /dice, in German /würfel.
fn dice() -> Command {
    let expression = StringBuilder::new(
        "expression",
        Text::DiceExpressionDescription.render(Lang::En),
    )
    .name_localizations([(GERMAN, "ausdruck")])
    .description_localizations([(GERMAN, Text::DiceExpressionDescription.render(Lang::De))])
    .max_length(u16::from(dice::MAX_LENGTH));
    let secret = BooleanBuilder::new("secret", Text::DiceSecretDescription.render(Lang::En))
        .name_localizations([(GERMAN, "geheim")])
        .description_localizations([(GERMAN, Text::DiceSecretDescription.render(Lang::De))]);
    CommandBuilder::new(
        "dice",
        Text::DiceDescription.render(Lang::En),
        CommandType::ChatInput,
    )
    .name_localizations([(GERMAN, "würfel")])
    .description_localizations([(GERMAN, Text::DiceDescription.render(Lang::De))])
    .contexts([InteractionContextType::Guild])
    .option(expression)
    .option(secret)
    .build()
}

/// `option` with a choice for each language: its [`Lang::code`], named in English and German.
fn languages(option: StringBuilder) -> StringBuilder {
    let name = |lang, of| Text::LanguageName { lang }.render(of);
    let choices = Lang::ALL.map(|lang| (name(lang, Lang::En), lang.code()));
    Lang::ALL
        .into_iter()
        .fold(option.choices(choices), |option, lang| {
            option.choice_localizations(&name(lang, Lang::En), [(GERMAN, name(lang, Lang::De))])
        })
}

/// What an interaction asks PuppyBot for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request<'a> {
    /// An answer with this text.
    Answer(Text<'static>),
    /// The language of the server: /settings language without `to`.
    ShowLanguage(Id<GuildMarker>),
    /// A new language for the server: /settings language with `to`.
    SetLanguage(Id<GuildMarker>, Lang),
    /// The prefix of the server: /settings prefix without `to`.
    ShowPrefix(Id<GuildMarker>),
    /// A new prefix for the server, which [`text_command::is_valid_prefix`] accepts: /settings
    /// prefix with `to`.
    SetPrefix(Id<GuildMarker>, &'a str),
    /// A roll of `dice`, which only the asker sees if it is `secret`: /dice.
    Roll {
        /// What to roll.
        dice: Dice,
        /// Whether only the asker sees the result.
        secret: bool,
    },
}

/// Who sees an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audience {
    /// Only the person who asked.
    Asker,
    /// Everyone in the channel.
    Everyone,
}

/// A slash command: what it asks for, and the language of the person who asked, in which the
/// answer is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked<'a> {
    /// What it asks for.
    pub request: Request<'a>,
    /// The language of the answer.
    pub lang: Lang,
}

/// What `interaction` asks for if it is a slash command, on one of `guilds` or not.
#[must_use]
pub fn request<'a>(
    interaction: &'a Interaction,
    guilds: &BTreeSet<Id<GuildMarker>>,
) -> Option<Asked<'a>> {
    let Some(InteractionData::ApplicationCommand(command)) = &interaction.data else {
        return None;
    };
    if interaction.kind != InteractionType::ApplicationCommand {
        return None;
    }
    let request = match interaction.guild_id {
        Some(guild) if guilds.contains(&guild) => match command.name.as_str() {
            "info" => Request::Answer(Text::Info {
                version: crate::VERSION,
            }),
            "settings" if may_change_settings(interaction) => settings(command, guild),
            "settings" => Request::Answer(Text::NotAllowed),
            "dice" => roll(command),
            _ => Request::Answer(Text::UnknownCommand),
        },
        _ => Request::Answer(Text::NotHere),
    };
    let lang = Lang::from_locale(interaction.locale.as_deref().unwrap_or_default());
    Some(Asked { request, lang })
}

/// Whether the person who sent `interaction` may manage its server, as Discord says, also
/// through the permission Administrator.
fn may_change_settings(interaction: &Interaction) -> bool {
    interaction
        .member
        .as_ref()
        .and_then(|member| member.permissions)
        .is_some_and(|permissions| {
            permissions.intersects(MAY_CHANGE_SETTINGS | Permissions::ADMINISTRATOR)
        })
}

/// /settings on `guild`: its subcommands language and prefix, with the option `to` or without.
/// Any other shape is of another version of the command.
fn settings(command: &CommandData, guild: Id<GuildMarker>) -> Request<'_> {
    let unknown = Request::Answer(Text::UnknownCommand);
    let [
        CommandDataOption {
            name,
            value: CommandOptionValue::SubCommand(options),
        },
    ] = command.options.as_slice()
    else {
        return unknown;
    };
    let to = match options.as_slice() {
        [] => None,
        [
            CommandDataOption {
                name,
                value: CommandOptionValue::String(value),
            },
        ] if name == "to" => Some(value.as_str()),
        _ => return unknown,
    };
    match (name.as_str(), to) {
        ("language", None) => Request::ShowLanguage(guild),
        ("language", Some(code)) => {
            Lang::from_code(code).map_or(unknown, |lang| Request::SetLanguage(guild, lang))
        }
        ("prefix", None) => Request::ShowPrefix(guild),
        ("prefix", Some(prefix)) if text_command::is_valid_prefix(prefix) => {
            Request::SetPrefix(guild, prefix)
        }
        ("prefix", Some(_)) => Request::Answer(Text::PrefixInvalid),
        _ => unknown,
    }
}

/// /dice: the expression, one die with six sides without it, and whether the result is
/// secret. Any other option is of another version of the command.
fn roll(command: &CommandData) -> Request<'static> {
    let mut expression = "";
    let mut secret = false;
    for option in &command.options {
        match (option.name.as_str(), &option.value) {
            ("expression", CommandOptionValue::String(text)) => expression = text,
            ("secret", CommandOptionValue::Boolean(value)) => secret = *value,
            _ => return Request::Answer(Text::UnknownCommand),
        }
    }
    match Dice::parse_or_d6(expression) {
        Ok(dice) => Request::Roll { dice, secret },
        Err(error) => Request::Answer(Text::dice_error(error)),
    }
}

/// The answer with `content` for `audience`, which mentions nobody.
#[must_use]
pub fn response(content: String, audience: Audience) -> InteractionResponse {
    let message = ChannelMessageBuilder::new()
        .content(content)
        .allowed_mentions(AllowedMentions::default());
    match audience {
        Audience::Asker => message.flags(MessageFlags::EPHEMERAL),
        Audience::Everyone => message,
    }
    .build()
}

#[cfg(test)]
pub(crate) mod tests {
    use twilight_model::application::command::{
        CommandOptionChoice, CommandOptionChoiceValue, CommandOptionType,
    };
    use twilight_model::http::interaction::InteractionResponseType;

    use super::*;

    /// The server of the tests.
    pub(crate) const GUILD: u64 = 1_100_000_000_000_000_001;

    /// The permission "Manage Server", as Discord sends permissions: a number.
    pub(crate) const MANAGE_GUILD: u64 = 1 << 5;

    /// The JSON of the options of /settings language to `code`.
    pub(crate) fn language_to(code: &str) -> String {
        format!(
            r#"[{{"name": "language", "type": 1, "options": [{{"name": "to", "type": 3, "value": "{code}"}}]}}]"#
        )
    }

    /// The JSON of the options of /settings language without `to`.
    pub(crate) const LANGUAGE: &str = r#"[{"name": "language", "type": 1, "options": []}]"#;

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

    /// /settings with the options `options`, as JSON, on the server [`GUILD`], by someone with
    /// the locale `locale` and the permissions `permissions`.
    pub(crate) fn settings(options: &str, locale: &str, permissions: u64) -> Interaction {
        command("settings", options, locale, permissions)
    }

    /// The slash command `name` with the options `options`, as JSON, on the server [`GUILD`], by
    /// someone with the locale `locale` and the permissions `permissions`.
    pub(crate) fn command(
        name: &str,
        options: &str,
        locale: &str,
        permissions: u64,
    ) -> Interaction {
        serde_json::from_str(&format!(
            r#"{{
                "id": "1200000000000000001",
                "application_id": "1200000000000000002",
                "type": 2,
                "token": "interaction-token",
                "version": 1,
                "locale": "{locale}",
                "attachment_size_limit": 26214400,
                "authorizing_integration_owners": {{}},
                "entitlements": [],
                "guild_id": "{GUILD}",
                "member": {{
                    "deaf": false, "mute": false, "flags": 0, "roles": [], "joined_at": null,
                    "permissions": "{permissions}",
                    "user": {{"id": "1400000000000000005", "username": "puppy",
                        "discriminator": "0", "avatar": null}}
                }},
                "data": {{"id": "1200000000000000004", "name": "{name}", "type": 1,
                    "options": {options}}}
            }}"#
        ))
        .unwrap()
    }

    fn guilds() -> BTreeSet<Id<GuildMarker>> {
        BTreeSet::from([Id::new(GUILD)])
    }

    /// What `interaction` asks for, in which language.
    fn asked(interaction: &Interaction) -> (Request<'_>, Lang) {
        let asked = request(interaction, &guilds()).unwrap();
        (asked.request, asked.lang)
    }

    #[test]
    fn info_in_the_language_of_the_person() {
        let info = Request::Answer(Text::Info {
            version: crate::VERSION,
        });
        for (locale, lang) in [("de", Lang::De), ("en-US", Lang::En), ("fr", Lang::En)] {
            let interaction = interaction(2, "info", locale, Some(GUILD));
            assert_eq!(asked(&interaction), (info.clone(), lang), "{locale}");
        }
    }

    #[test]
    fn commands_it_does_not_know() {
        let interaction = interaction(2, "dance", "en-GB", Some(GUILD));
        assert_eq!(
            asked(&interaction),
            (Request::Answer(Text::UnknownCommand), Lang::En)
        );
    }

    #[test]
    fn commands_outside_its_servers() {
        for guild in [Some(GUILD + 1), None] {
            let interaction = interaction(2, "info", "de", guild);
            assert_eq!(
                asked(&interaction),
                (Request::Answer(Text::NotHere), Lang::De)
            );
        }
    }

    #[test]
    fn no_answer_but_to_slash_commands() {
        // Autocomplete has the data of a command, but isn't one.
        let autocomplete = interaction(4, "info", "de", Some(GUILD));
        assert_eq!(request(&autocomplete, &guilds()), None);
        let mut without_data = interaction(2, "info", "de", Some(GUILD));
        without_data.data = None;
        assert_eq!(request(&without_data, &guilds()), None);
    }

    #[test]
    fn the_language_of_the_server() {
        let guild = Id::new(GUILD);
        assert_eq!(
            asked(&settings(LANGUAGE, "de", MANAGE_GUILD)),
            (Request::ShowLanguage(guild), Lang::De)
        );
        for (code, lang) in [("de", Lang::De), ("en", Lang::En)] {
            assert_eq!(
                asked(&settings(&language_to(code), "en-US", MANAGE_GUILD)),
                (Request::SetLanguage(guild, lang), Lang::En)
            );
        }
        // Administrators may, too, also without the permission Manage Server.
        let administrator = 1 << 3;
        assert_eq!(
            asked(&settings(LANGUAGE, "de", administrator)).0,
            Request::ShowLanguage(guild)
        );
    }

    #[test]
    fn the_prefix_of_the_server() {
        let guild = Id::new(GUILD);
        let prefix = |to: Option<&str>| {
            let to = to.map_or_else(String::new, |to| {
                format!(r#"{{"name": "to", "type": 3, "value": "{to}"}}"#)
            });
            settings(
                &format!(r#"[{{"name": "prefix", "type": 1, "options": [{to}]}}]"#),
                "de",
                MANAGE_GUILD,
            )
        };
        let show = prefix(None);
        assert_eq!(asked(&show).0, Request::ShowPrefix(guild));
        let set = prefix(Some("pb?"));
        assert_eq!(asked(&set).0, Request::SetPrefix(guild, "pb?"));
        // Discord's limits of the option let these through, PuppyBot doesn't.
        for invalid in ["a b", "<@1>", "/", "ü"] {
            let set = prefix(Some(invalid));
            assert_eq!(
                asked(&set).0,
                Request::Answer(Text::PrefixInvalid),
                "{invalid}"
            );
        }
    }

    #[test]
    fn settings_only_for_people_who_may_manage_the_server() {
        let send_messages = 1 << 11;
        for permissions in [0, send_messages] {
            assert_eq!(
                asked(&settings(&language_to("de"), "de", permissions)),
                (Request::Answer(Text::NotAllowed), Lang::De)
            );
        }
        let mut without_member = settings(LANGUAGE, "de", MANAGE_GUILD);
        without_member.member = None;
        assert_eq!(asked(&without_member).0, Request::Answer(Text::NotAllowed));
    }

    #[test]
    fn settings_of_other_versions() {
        for options in [
            "[]".to_owned(),
            r#"[{"name": "colour", "type": 1, "options": []}]"#.to_owned(),
            r#"[{"name": "colour", "type": 1, "options": [{"name": "to", "type": 3, "value": "red"}]}]"#.to_owned(),
            language_to("tlh"),
            r#"[{"name": "language", "type": 1, "options": [{"name": "from", "type": 3, "value": "de"}]}]"#.to_owned(),
            r#"[{"name": "language", "type": 1, "options": [{"name": "to", "type": 4, "value": 1}]}]"#.to_owned(),
        ] {
            assert_eq!(
                asked(&settings(&options, "de", MANAGE_GUILD)).0,
                Request::Answer(Text::UnknownCommand),
                "{options}"
            );
        }
    }

    #[test]
    fn dice() {
        let dice = |options: &str| command("dice", options, "de", 0);
        assert_eq!(
            asked(&dice("[]")).0,
            Request::Roll {
                dice: Dice::d6(),
                secret: false
            }
        );
        let options = r#"[{"name": "expression", "type": 3, "value": "2W6+3"},
            {"name": "secret", "type": 5, "value": true}]"#;
        assert_eq!(
            asked(&dice(options)).0,
            Request::Roll {
                dice: Dice::parse("2d6+3").unwrap(),
                secret: true
            }
        );
        for (options, answer) in [
            (
                r#"[{"name": "expression", "type": 3, "value": "2d"}]"#,
                Text::DiceInvalid,
            ),
            (
                r#"[{"name": "expression", "type": 3, "value": "101d6"}]"#,
                Text::DiceTooLarge,
            ),
            (
                r#"[{"name": "colour", "type": 3, "value": "red"}]"#,
                Text::UnknownCommand,
            ),
            (
                r#"[{"name": "secret", "type": 3, "value": "yes"}]"#,
                Text::UnknownCommand,
            ),
        ] {
            assert_eq!(
                asked(&dice(options)).0,
                Request::Answer(answer),
                "{options}"
            );
        }
    }

    #[test]
    fn answers_for_everyone_in_the_channel() {
        let data = response("4".to_owned(), Audience::Everyone).data.unwrap();
        assert_eq!(data.flags, None);
        assert_eq!(data.allowed_mentions, Some(AllowedMentions::default()));
    }

    #[test]
    fn answers_only_for_the_person_who_asked() {
        let response = response(Text::NotHere.render(Lang::En), Audience::Asker);
        assert_eq!(
            response.kind,
            InteractionResponseType::ChannelMessageWithSource
        );
        let data = response.data.unwrap();
        assert_eq!(data.flags, Some(MessageFlags::EPHEMERAL));
        assert_eq!(data.allowed_mentions, Some(AllowedMentions::default()));
        assert_eq!(
            data.content.as_deref(),
            Some("PuppyBot works only on its servers.")
        );
    }

    #[test]
    fn the_commands_in_both_languages() {
        let commands = commands();
        let names: Vec<&str> = commands
            .iter()
            .map(|command| command.name.as_str())
            .collect();
        assert_eq!(names, ["info", "settings", "dice"]);
        for command in &commands {
            let german = &command.description_localizations.as_ref().unwrap()["de"];
            assert_ne!(&command.description, german);
            assert_eq!(command.contexts, Some(vec![InteractionContextType::Guild]));
        }
        let (info, settings, dice) = (&commands[0], &commands[1], &commands[2]);
        // /dice, for everyone, in German /würfel.
        assert_eq!(dice.default_member_permissions, None);
        let german = |names: &Option<std::collections::HashMap<String, String>>| {
            names.as_ref().and_then(|names| names.get("de").cloned())
        };
        assert_eq!(german(&dice.name_localizations).as_deref(), Some("würfel"));
        let options: Vec<_> = dice
            .options
            .iter()
            .map(|option| {
                (
                    option.name.as_str(),
                    option.kind,
                    german(&option.name_localizations),
                    option.max_length,
                    option.required,
                )
            })
            .collect();
        assert_eq!(
            options,
            [
                (
                    "expression",
                    CommandOptionType::String,
                    Some("ausdruck".to_owned()),
                    Some(100),
                    None
                ),
                (
                    "secret",
                    CommandOptionType::Boolean,
                    Some("geheim".to_owned()),
                    None,
                    None
                ),
            ]
        );
        assert_eq!(
            info.description,
            "Shows what PuppyBot is and which version runs"
        );
        assert_eq!(info.default_member_permissions, None);
        assert_eq!(
            settings.default_member_permissions,
            Some(Permissions::MANAGE_GUILD)
        );

        assert_eq!(settings.options.len(), 2);
        let (language, prefix) = (&settings.options[0], &settings.options[1]);
        assert_eq!(
            (language.name.as_str(), language.kind),
            ("language", CommandOptionType::SubCommand)
        );
        let options = language.options.as_ref().unwrap();
        assert_eq!(options.len(), 1);
        let to = &options[0];
        assert_eq!(
            (to.name.as_str(), to.kind, to.required),
            ("to", CommandOptionType::String, None)
        );
        let choice = |name: &str, german: &str, code: &str| CommandOptionChoice {
            name: name.to_owned(),
            name_localizations: Some([("de".to_owned(), german.to_owned())].into()),
            value: CommandOptionChoiceValue::String(code.to_owned()),
        };
        assert_eq!(
            to.choices,
            Some(vec![
                choice("German", "Deutsch", "de"),
                choice("English", "Englisch", "en"),
            ])
        );

        // /settings prefix, whose option Discord already limits to 1 to 5 characters.
        assert_eq!(
            (prefix.name.as_str(), prefix.kind),
            ("prefix", CommandOptionType::SubCommand)
        );
        let options = prefix.options.as_ref().unwrap();
        assert_eq!(options.len(), 1);
        let to = &options[0];
        assert_eq!(
            (to.name.as_str(), to.min_length, to.max_length),
            ("to", Some(1), Some(5))
        );
        // Any text, no choices.
        assert!(to.choices.as_ref().is_none_or(Vec::is_empty));
    }
}
