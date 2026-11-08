//! The slash commands of PuppyBot on Discord: their definitions, what an interaction asks for,
//! and the answers. The texts are typed texts of `puppybot-core`, in German and English.

use std::collections::BTreeSet;

use puppybot_core::dice::{self, Dice};
use puppybot_core::games::{GameText, Guessing, Hand};
use puppybot_core::i18n::Lang;
use puppybot_core::poll::{self, Poll};
use puppybot_core::quotes::QuoteText;
use puppybot_core::text::Text;
use puppybot_core::text_command;
use twilight_model::application::command::{Command, CommandType};
use twilight_model::application::interaction::application_command::{
    CommandData, CommandDataOption, CommandOptionValue,
};
use twilight_model::application::interaction::{
    Interaction, InteractionContextType, InteractionData, InteractionType,
};
use twilight_model::channel::message::component::{ActionRow, Button, ButtonStyle};
use twilight_model::channel::message::{AllowedMentions, Component, MessageFlags};
use twilight_model::guild::Permissions;
use twilight_model::http::interaction::{InteractionResponse, InteractionResponseType};
use twilight_model::id::Id;
use twilight_model::id::marker::GuildMarker;
use twilight_model::util::Timestamp;
use twilight_util::builder::command::{
    BooleanBuilder, CommandBuilder, IntegerBuilder, StringBuilder, SubCommandBuilder, UserBuilder,
};
use twilight_util::builder::interaction_response::ChannelMessageBuilder;

use crate::storage::Quote;

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
        poll(),
        CommandBuilder::new(
            "server",
            Text::ServerDescription.render(Lang::En),
            CommandType::ChatInput,
        )
        .description_localizations([(GERMAN, Text::ServerDescription.render(Lang::De))])
        .contexts([InteractionContextType::Guild])
        .build(),
        CommandBuilder::new(
            "user",
            Text::UserDescription.render(Lang::En),
            CommandType::ChatInput,
        )
        .name_localizations([(GERMAN, "nutzer")])
        .description_localizations([(GERMAN, Text::UserDescription.render(Lang::De))])
        .contexts([InteractionContextType::Guild])
        .option(
            UserBuilder::new("who", Text::UserWhoDescription.render(Lang::En))
                .name_localizations([(GERMAN, "wer")])
                .description_localizations([(GERMAN, Text::UserWhoDescription.render(Lang::De))]),
        )
        .build(),
        game("coin", "münze", GameText::CoinDescription).build(),
        game("8ball", "8kugel", GameText::EightBallDescription)
            .option(
                StringBuilder::new(
                    "question",
                    GameText::EightBallQuestionDescription.render(Lang::En),
                )
                .name_localizations([(GERMAN, "frage")])
                .description_localizations([(
                    GERMAN,
                    GameText::EightBallQuestionDescription.render(Lang::De),
                )])
                .max_length(u16::try_from(poll::MAX_QUESTION).unwrap_or(u16::MAX)),
            )
            .build(),
        game("rps", "ssp", GameText::RpsDescription).build(),
        game("guess", "raten", GameText::GuessDescription)
            .option(
                IntegerBuilder::new("number", GameText::GuessNumberDescription.render(Lang::En))
                    .name_localizations([(GERMAN, "zahl")])
                    .description_localizations([(
                        GERMAN,
                        GameText::GuessNumberDescription.render(Lang::De),
                    )])
                    .min_value(1)
                    .max_value(i64::from(Guessing::MAX)),
            )
            .option(
                IntegerBuilder::new("max", GameText::GuessMaxDescription.render(Lang::En))
                    .name_localizations([(GERMAN, "bis")])
                    .description_localizations([(
                        GERMAN,
                        GameText::GuessMaxDescription.render(Lang::De),
                    )])
                    .min_value(2)
                    .max_value(i64::from(Guessing::MAX)),
            )
            .build(),
        save_quote(),
        quote(),
    ]
}

/// The name of the command in the menu of messages, under Apps, that saves one as a quote.
pub const SAVE_QUOTE: &str = "Save quote";

/// Save quote, in German Zitat speichern, in the menu of messages. Such commands have no
/// description.
fn save_quote() -> Command {
    CommandBuilder::new(SAVE_QUOTE, "", CommandType::Message)
        .name_localizations([(GERMAN, "Zitat speichern")])
        .contexts([InteractionContextType::Guild])
        .build()
}

/// /quote, in German /zitat: show, with the number of a quote or without, and delete, with it.
fn quote() -> Command {
    let number = |required| {
        IntegerBuilder::new("number", QuoteText::NumberDescription.render(Lang::En))
            .name_localizations([(GERMAN, "nummer")])
            .description_localizations([(GERMAN, QuoteText::NumberDescription.render(Lang::De))])
            .min_value(1)
            .max_value(i64::from(u32::MAX))
            .required(required)
    };
    let show = SubCommandBuilder::new("show", QuoteText::ShowDescription.render(Lang::En))
        .name_localizations([(GERMAN, "zeigen")])
        .description_localizations([(GERMAN, QuoteText::ShowDescription.render(Lang::De))])
        .option(number(false));
    let delete = SubCommandBuilder::new("delete", QuoteText::DeleteDescription.render(Lang::En))
        .name_localizations([(GERMAN, "löschen")])
        .description_localizations([(GERMAN, QuoteText::DeleteDescription.render(Lang::De))])
        .option(number(true));
    CommandBuilder::new(
        "quote",
        QuoteText::QuoteDescription.render(Lang::En),
        CommandType::ChatInput,
    )
    .name_localizations([(GERMAN, "zitat")])
    .description_localizations([(GERMAN, QuoteText::QuoteDescription.render(Lang::De))])
    .contexts([InteractionContextType::Guild])
    .option(show)
    .option(delete)
    .build()
}

/// The slash command of a game, `name`, in German `german`, with its `description`.
fn game(name: &str, german: &str, description: GameText<'_>) -> CommandBuilder {
    CommandBuilder::new(name, description.render(Lang::En), CommandType::ChatInput)
        .name_localizations([(GERMAN, german)])
        .description_localizations([(GERMAN, description.render(Lang::De))])
        .contexts([InteractionContextType::Guild])
}

/// The prefix of the IDs of the buttons of rock, paper, scissors, before the hand's.
pub const RPS: &str = "rps:";

/// When the thing with the ID `id` came to Discord, in seconds since 1970: Discord's IDs begin
/// with the milliseconds since 2015.
#[must_use]
pub fn created(id: u64) -> i64 {
    // At most 2^42 milliseconds, which an i64 holds.
    (i64::try_from(id >> 22).unwrap_or(0) + 1_420_070_400_000) / 1000
}

/// /poll, in German /umfrage: the question, 2 to 10 answers, the hours, and whether people may
/// choose more than one answer. Discord wants the options that one must give first.
fn poll() -> Command {
    let mut command = CommandBuilder::new(
        "poll",
        Text::PollDescription.render(Lang::En),
        CommandType::ChatInput,
    )
    .name_localizations([(GERMAN, "umfrage")])
    .description_localizations([(GERMAN, Text::PollDescription.render(Lang::De))])
    .contexts([InteractionContextType::Guild])
    .option(
        StringBuilder::new("question", Text::PollQuestionDescription.render(Lang::En))
            .name_localizations([(GERMAN, "frage")])
            .description_localizations([(GERMAN, Text::PollQuestionDescription.render(Lang::De))])
            .max_length(u16::try_from(poll::MAX_QUESTION).unwrap_or(u16::MAX))
            .required(true),
    );
    for number in 1..=poll::MAX_ANSWERS {
        command = command.option(
            StringBuilder::new(
                format!("answer{number}"),
                Text::PollAnswerDescription.render(Lang::En),
            )
            .name_localizations([(GERMAN, format!("antwort{number}"))])
            .description_localizations([(GERMAN, Text::PollAnswerDescription.render(Lang::De))])
            .max_length(u16::try_from(poll::MAX_ANSWER).unwrap_or(u16::MAX))
            .required(number <= poll::MIN_ANSWERS),
        );
    }
    command
        .option(
            IntegerBuilder::new("hours", Text::PollHoursDescription.render(Lang::En))
                .name_localizations([(GERMAN, "stunden")])
                .description_localizations([(GERMAN, Text::PollHoursDescription.render(Lang::De))])
                .min_value(1)
                .max_value(i64::from(poll::MAX_HOURS)),
        )
        .option(
            BooleanBuilder::new("multiple", Text::PollMultipleDescription.render(Lang::En))
                .name_localizations([(GERMAN, "mehrere")])
                .description_localizations([(
                    GERMAN,
                    Text::PollMultipleDescription.render(Lang::De),
                )]),
        )
        .build()
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
    Answer(Text<'a>),
    /// The language of the server: /settings language without `to`.
    ShowLanguage(Id<GuildMarker>),
    /// A new language for the server: /settings language with `to`.
    SetLanguage(Id<GuildMarker>, Lang),
    /// The prefix of the server: /settings prefix without `to`.
    ShowPrefix(Id<GuildMarker>),
    /// A new prefix for the server, which [`text_command::is_valid_prefix`] accepts: /settings
    /// prefix with `to`.
    SetPrefix(Id<GuildMarker>, &'a str),
    /// A poll to post in the channel: /poll.
    Poll(Poll),
    /// When the server was created, and how many members it has: /server.
    ServerInfo(Id<GuildMarker>),
    /// A coin: /coin.
    Coin,
    /// The magic 8-ball, with the question: /8ball.
    EightBall(&'a str),
    /// The buttons of rock, paper, scissors: /rps.
    RpsChoose,
    /// A game of rock, paper, scissors: someone clicked a button.
    RpsPlay {
        /// Their hand.
        hand: Hand,
        /// Their name.
        player: &'a str,
    },
    /// Guessing a number: /guess.
    Guess {
        /// The guess, none to start a game, or to see the one that runs.
        number: Option<u32>,
        /// The largest number of a new game.
        max: Option<u32>,
        /// The name of the person who guessed.
        player: &'a str,
    },
    /// A message to save as a quote on the server: Save quote, in the menu of a message.
    SaveQuote(Id<GuildMarker>, Quote),
    /// The quote of the server with this number, or one at random: /quote show.
    ShowQuote(Id<GuildMarker>, Option<u32>),
    /// Deleting a quote of the server: /quote delete.
    DeleteQuote {
        /// The server.
        guild: Id<GuildMarker>,
        /// The number of the quote.
        number: u32,
        /// The ID of the person who asked.
        asker: u64,
        /// Whether they may manage messages, and so delete any quote.
        may_manage: bool,
    },
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
    let on_server = interaction.guild_id.filter(|guild| guilds.contains(guild));
    let request = match (&interaction.data, interaction.kind, on_server) {
        (
            Some(InteractionData::ApplicationCommand(command)),
            InteractionType::ApplicationCommand,
            Some(guild),
        ) => slash(interaction, command, guild),
        (
            Some(InteractionData::MessageComponent(component)),
            InteractionType::MessageComponent,
            Some(_),
        ) => {
            let hand = component
                .custom_id
                .strip_prefix(RPS)
                .and_then(Hand::from_id)?;
            Request::RpsPlay {
                hand,
                player: name(interaction),
            }
        }
        (
            Some(InteractionData::ApplicationCommand(_) | InteractionData::MessageComponent(_)),
            InteractionType::ApplicationCommand | InteractionType::MessageComponent,
            None,
        ) => Request::Answer(Text::NotHere),
        _ => return None,
    };
    let lang = Lang::from_locale(interaction.locale.as_deref().unwrap_or_default());
    Some(Asked { request, lang })
}

/// The name of the person who sent `interaction`.
fn name(interaction: &Interaction) -> &str {
    interaction
        .member
        .as_ref()
        .and_then(|member| member.user.as_ref())
        .map_or("?", |user| {
            user.global_name.as_deref().unwrap_or(&user.name)
        })
}

/// What the slash command `command` on `guild` asks for.
fn slash<'a>(
    interaction: &'a Interaction,
    command: &'a CommandData,
    guild: Id<GuildMarker>,
) -> Request<'a> {
    match command.name.as_str() {
        "info" => Request::Answer(Text::Info {
            version: crate::VERSION,
        }),
        "settings" if may(interaction, MAY_CHANGE_SETTINGS) => settings(command, guild),
        "settings" => Request::Answer(Text::NotAllowed),
        "dice" => roll(command),
        "poll" => poll_request(command),
        "server" => Request::ServerInfo(guild),
        "user" => user_info(interaction, command),
        "coin" => Request::Coin,
        "8ball" => Request::EightBall(string(command, "question").unwrap_or_default()),
        "rps" => Request::RpsChoose,
        "guess" => Request::Guess {
            number: number(&command.options, "number"),
            max: number(&command.options, "max"),
            player: name(interaction),
        },
        SAVE_QUOTE => quote_to_save(command, guild),
        "quote" => quote_request(interaction, command, guild),
        _ => Request::Answer(Text::UnknownCommand),
    }
}

/// The text of the option `name` of `command`, none if it has none.
fn string<'a>(command: &'a CommandData, name: &str) -> Option<&'a str> {
    command
        .options
        .iter()
        .find_map(|option| match &option.value {
            CommandOptionValue::String(text) if option.name == name => Some(text.as_str()),
            _ => None,
        })
}

/// The whole number of the option `name` among `options`, none if there is none, or a negative
/// one.
fn number(options: &[CommandDataOption], name: &str) -> Option<u32> {
    options.iter().find_map(|option| match option.value {
        CommandOptionValue::Integer(value) if option.name == name => u32::try_from(value).ok(),
        _ => None,
    })
}

/// Whether the person who sent `interaction` has `permission` there, as Discord says, also
/// through the permission Administrator.
fn may(interaction: &Interaction, permission: Permissions) -> bool {
    interaction
        .member
        .as_ref()
        .and_then(|member| member.permissions)
        .is_some_and(|permissions| permissions.intersects(permission | Permissions::ADMINISTRATOR))
}

/// Save quote on `guild`: the message, which Discord sends with the command. One that isn't
/// there counts as one without text.
fn quote_to_save(command: &CommandData, guild: Id<GuildMarker>) -> Request<'static> {
    let message = command
        .target_id
        .and_then(|target| command.resolved.as_ref()?.messages.get(&target.cast()));
    let quote = message.map_or_else(Quote::default, |message| {
        let author = &message.author;
        Quote {
            author: author.id.get(),
            name: author
                .global_name
                .as_deref()
                .unwrap_or(&author.name)
                .to_owned(),
            channel: message.channel_id.get(),
            message: message.id.get(),
            text: message.content.clone(),
        }
    });
    Request::SaveQuote(guild, quote)
}

/// /quote on `guild`: show, with the number of a quote or without, and delete, with it. Any
/// other shape is of another version of the command.
fn quote_request(
    interaction: &Interaction,
    command: &CommandData,
    guild: Id<GuildMarker>,
) -> Request<'static> {
    let [
        CommandDataOption {
            name,
            value: CommandOptionValue::SubCommand(options),
        },
    ] = command.options.as_slice()
    else {
        return Request::Answer(Text::UnknownCommand);
    };
    match (name.as_str(), number(options, "number")) {
        ("show", number) => Request::ShowQuote(guild, number),
        ("delete", Some(number)) => Request::DeleteQuote {
            guild,
            number,
            asker: interaction.author_id().map_or(0, Id::get),
            may_manage: may(interaction, Permissions::MANAGE_MESSAGES),
        },
        _ => Request::Answer(Text::UnknownCommand),
    }
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

/// /user: since when the person in the option `who`, or who asked, is on Discord and on the
/// server. Discord sends the person with the command.
fn user_info<'a>(interaction: &'a Interaction, command: &'a CommandData) -> Request<'a> {
    let who =
        command
            .options
            .iter()
            .find_map(|option| match (option.name.as_str(), &option.value) {
                ("who", CommandOptionValue::User(user)) => Some(*user),
                _ => None,
            });
    let (user, joined) = if let Some(who) = who {
        let resolved = command.resolved.as_ref();
        let user = resolved.and_then(|resolved| resolved.users.get(&who));
        let member = resolved.and_then(|resolved| resolved.members.get(&who));
        (user, member.and_then(|member| member.joined_at))
    } else {
        let member = interaction.member.as_ref();
        let user = member.and_then(|member| member.user.as_ref());
        (user, member.and_then(|member| member.joined_at))
    };
    let Some(user) = user else {
        return Request::Answer(Text::UnknownCommand);
    };
    Request::Answer(Text::UserInfo {
        name: user.global_name.as_deref().unwrap_or(&user.name),
        created: created(user.id.get()),
        joined: joined.map(Timestamp::as_secs),
    })
}

/// /poll: the question, the answers in the order of their numbers, the hours, and whether
/// people may choose more than one answer. Any other option is of another version of the
/// command.
fn poll_request(command: &CommandData) -> Request<'static> {
    let mut question = "";
    let mut answers = Vec::new();
    let mut hours = poll::DEFAULT_HOURS;
    let mut multiple = false;
    for option in &command.options {
        let number = option
            .name
            .strip_prefix("answer")
            .and_then(|number| number.parse::<u8>().ok());
        match (option.name.as_str(), number, &option.value) {
            ("question", _, CommandOptionValue::String(text)) => question = text,
            (_, Some(number), CommandOptionValue::String(text)) => {
                answers.push((number, text.as_str()));
            }
            // Beyond the limits of Discord's option: 0, which no poll has.
            ("hours", _, CommandOptionValue::Integer(value)) => {
                hours = u16::try_from(*value).unwrap_or(0);
            }
            ("multiple", _, CommandOptionValue::Boolean(value)) => multiple = *value,
            _ => return Request::Answer(Text::UnknownCommand),
        }
    }
    answers.sort_by_key(|&(number, _)| number);
    let answers: Vec<&str> = answers.into_iter().map(|(_, text)| text).collect();
    match Poll::new(question, &answers, hours, multiple) {
        Ok(poll) => Request::Poll(poll),
        Err(error) => Request::Answer(Text::poll_error(error)),
    }
}

/// An answer of PuppyBot to an interaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    /// The text.
    pub content: String,
    /// Who sees it.
    pub audience: Audience,
    /// Buttons under the text: their IDs and labels.
    pub buttons: Vec<(String, String)>,
    /// Whether it takes the place of the message whose button someone clicked.
    pub update: bool,
}

impl Answer {
    /// An answer with `content` for `audience`, without buttons.
    #[must_use]
    pub const fn new(content: String, audience: Audience) -> Self {
        Self {
            content,
            audience,
            buttons: Vec::new(),
            update: false,
        }
    }
}

/// The response with `answer`, which mentions nobody.
#[must_use]
pub fn response(answer: Answer) -> InteractionResponse {
    let message = ChannelMessageBuilder::new()
        .content(answer.content)
        .allowed_mentions(AllowedMentions::default())
        .components(buttons(&answer.buttons));
    let mut response = match answer.audience {
        Audience::Asker => message.flags(MessageFlags::EPHEMERAL),
        Audience::Everyone => message,
    }
    .build();
    if answer.update {
        response.kind = InteractionResponseType::UpdateMessage;
    }
    response
}

/// A row of buttons with these IDs and labels, none if there are none.
#[must_use]
pub fn buttons(buttons: &[(String, String)]) -> Vec<Component> {
    if buttons.is_empty() {
        return Vec::new();
    }
    let row = buttons
        .iter()
        .map(|(id, label)| {
            Component::Button(Button {
                custom_id: Some(id.clone()),
                disabled: false,
                emoji: None,
                id: None,
                label: Some(label.clone()),
                sku_id: None,
                style: ButtonStyle::Primary,
                url: None,
            })
        })
        .collect();
    vec![Component::ActionRow(ActionRow {
        components: row,
        id: None,
    })]
}

/// The buttons of rock, paper, scissors, labeled in `lang`.
#[must_use]
pub fn rps_buttons(lang: Lang) -> Vec<(String, String)> {
    Hand::ALL
        .into_iter()
        .map(|hand| {
            (
                format!("{RPS}{}", hand.id()),
                GameText::Hand(hand).render(lang),
            )
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use twilight_model::application::command::{
        CommandOptionChoice, CommandOptionChoiceValue, CommandOptionType,
        CommandOptionValue as Limit,
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
        resolved(name, options, "null", locale, permissions)
    }

    /// [`command`], with `resolved`, as JSON: what Discord sends of the people, channels or
    /// messages in the options.
    pub(crate) fn resolved(
        name: &str,
        options: &str,
        resolved: &str,
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
                    "options": {options}, "resolved": {resolved}}}
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
    fn polls() {
        let poll = |options: &str| command("poll", options, "de", 0);
        // The answers in the order of their numbers, whatever the order of the options.
        let options = r#"[{"name": "question", "type": 3, "value": "Pizza?"},
            {"name": "answer10", "type": 3, "value": "Zehn"},
            {"name": "answer2", "type": 3, "value": "Nein"},
            {"name": "answer1", "type": 3, "value": "Ja"},
            {"name": "hours", "type": 4, "value": 48},
            {"name": "multiple", "type": 5, "value": true}]"#;
        assert_eq!(
            asked(&poll(options)).0,
            Request::Poll(Poll::new("Pizza?", &["Ja", "Nein", "Zehn"], 48, true).unwrap())
        );
        let options = r#"[{"name": "question", "type": 3, "value": "Pizza?"},
            {"name": "answer1", "type": 3, "value": "Ja"},
            {"name": "answer2", "type": 3, "value": "Nein"}]"#;
        assert_eq!(
            asked(&poll(options)).0,
            Request::Poll(Poll::parse("Pizza? | Ja | Nein").unwrap())
        );
        for (options, answer) in [
            (
                r#"[{"name": "question", "type": 3, "value": "?"}, {"name": "answer1", "type": 3, "value": "Ja"}]"#,
                Text::PollInvalid,
            ),
            (
                r#"[{"name": "question", "type": 3, "value": "?"}, {"name": "answer1", "type": 3, "value": "a"},
                {"name": "answer2", "type": 3, "value": "b"}, {"name": "hours", "type": 4, "value": 1000000}]"#,
                Text::PollTooLarge,
            ),
            (
                r#"[{"name": "answerx", "type": 3, "value": "a"}]"#,
                Text::UnknownCommand,
            ),
            (
                r#"[{"name": "hours", "type": 3, "value": "24"}]"#,
                Text::UnknownCommand,
            ),
        ] {
            assert_eq!(
                asked(&poll(options)).0,
                Request::Answer(answer),
                "{options}"
            );
        }
    }

    #[test]
    fn the_server_and_people() {
        assert_eq!(
            asked(&command("server", "[]", "de", 0)).0,
            Request::ServerInfo(Id::new(GUILD))
        );
        // Who asked, without the option.
        assert_eq!(
            asked(&command("user", "[]", "de", 0)).0,
            Request::Answer(Text::UserInfo {
                name: "puppy",
                created: 1_753_856_410,
                joined: None
            })
        );
        // Someone else, whom Discord sends with the command.
        let who = r#"[{"name": "who", "type": 6, "value": "1100000000000000001"}]"#;
        let people = r#"{"users": {"1100000000000000001": {"id": "1100000000000000001",
            "username": "rex", "global_name": "Rex", "discriminator": "0", "avatar": null}},
            "members": {"1100000000000000001": {"roles": [], "flags": 0, "pending": false,
            "permissions": "0", "joined_at": "2026-01-01T00:00:00.000000+00:00"}}}"#;
        assert_eq!(
            asked(&resolved("user", who, people, "de", 0)).0,
            Request::Answer(Text::UserInfo {
                name: "Rex",
                created: 1_682_330_837,
                joined: Some(1_767_225_600)
            })
        );
        // Unless Discord didn't.
        assert_eq!(
            asked(&command("user", who, "de", 0)).0,
            Request::Answer(Text::UnknownCommand)
        );
        // An option of another version doesn't name anyone: who asked.
        let other = r#"[{"name": "colour", "type": 3, "value": "red"}]"#;
        assert_eq!(
            asked(&command("user", other, "de", 0)).0,
            Request::Answer(Text::UserInfo {
                name: "puppy",
                created: 1_753_856_410,
                joined: None
            })
        );
        let mut without_member = command("user", "[]", "de", 0);
        without_member.member = None;
        assert_eq!(
            asked(&without_member).0,
            Request::Answer(Text::UnknownCommand)
        );
    }

    #[test]
    fn when_things_came_to_discord() {
        // Discord's epoch, 2015, and the last millisecond that its IDs can hold.
        assert_eq!(created(0), 1_420_070_400);
        assert_eq!(created(u64::MAX), 5_818_116_911);
    }

    /// A click on the button `id` on the server `guild`, as Discord sends it.
    fn button(id: &str, guild: u64) -> Interaction {
        serde_json::from_str(&format!(
            r#"{{"id": "1200000000000000001", "application_id": "1200000000000000002", "type": 3,
            "token": "interaction-token", "version": 1, "locale": "de",
            "attachment_size_limit": 26214400, "authorizing_integration_owners": {{}},
            "entitlements": [], "guild_id": "{guild}",
            "member": {{"deaf": false, "mute": false, "flags": 0, "roles": [], "joined_at": null,
                "permissions": "0", "user": {{"id": "1400000000000000005", "username": "puppy",
                "global_name": "Puppy", "discriminator": "0", "avatar": null}}}},
            "data": {{"custom_id": "{id}", "component_type": 2}}}}"#
        ))
        .unwrap()
    }

    #[test]
    fn games() {
        let game = |name, options| command(name, options, "de", 0);
        assert_eq!(asked(&game("coin", "[]")).0, Request::Coin);
        let question = r#"[{"name": "question", "type": 3, "value": "Sonne?"}]"#;
        assert_eq!(
            asked(&game("8ball", question)).0,
            Request::EightBall("Sonne?")
        );
        assert_eq!(asked(&game("8ball", "[]")).0, Request::EightBall(""));
        // A question that isn't a text, which Discord doesn't send: as none.
        let question = r#"[{"name": "question", "type": 4, "value": 8}]"#;
        assert_eq!(asked(&game("8ball", question)).0, Request::EightBall(""));
        assert_eq!(asked(&game("rps", "[]")).0, Request::RpsChoose);
        let options = r#"[{"name": "number", "type": 4, "value": 42}, {"name": "max", "type": 4, "value": 50}]"#;
        assert_eq!(
            asked(&game("guess", options)).0,
            Request::Guess {
                number: Some(42),
                max: Some(50),
                player: "puppy"
            }
        );
        // Discord's limits don't allow negative numbers: as none.
        let options = r#"[{"name": "number", "type": 4, "value": -1}, {"name": "max", "type": 3, "value": "x"}]"#;
        assert_eq!(
            asked(&game("guess", options)).0,
            Request::Guess {
                number: None,
                max: None,
                player: "puppy"
            }
        );
    }

    #[test]
    fn buttons_of_rock_paper_scissors() {
        assert_eq!(
            asked(&button("rps:rock", GUILD)).0,
            Request::RpsPlay {
                hand: Hand::Rock,
                player: "Puppy"
            }
        );
        // Of no game, or outside the servers.
        assert_eq!(request(&button("rps:lizard", GUILD), &guilds()), None);
        assert_eq!(request(&button("other", GUILD), &guilds()), None);
        assert_eq!(
            asked(&button("rps:rock", GUILD + 1)).0,
            Request::Answer(Text::NotHere)
        );
        let mut nobody = button("rps:paper", GUILD);
        nobody.member = None;
        assert_eq!(
            asked(&nobody).0,
            Request::RpsPlay {
                hand: Hand::Paper,
                player: "?"
            }
        );
    }

    #[test]
    fn answers_with_buttons() {
        // The buttons, and the answer that takes the place of their message.
        let answer = Answer {
            buttons: rps_buttons(Lang::En),
            update: true,
            ..Answer::new("Rock, paper or scissors?".to_owned(), Audience::Everyone)
        };
        let response = response(answer);
        assert_eq!(response.kind, InteractionResponseType::UpdateMessage);
        let components = response.data.unwrap().components.unwrap();
        let button = |id: &str, label: &str| {
            Component::Button(Button {
                custom_id: Some(id.to_owned()),
                disabled: false,
                emoji: None,
                id: None,
                label: Some(label.to_owned()),
                sku_id: None,
                style: ButtonStyle::Primary,
                url: None,
            })
        };
        assert_eq!(
            components,
            [Component::ActionRow(ActionRow {
                components: vec![
                    button("rps:rock", "Rock"),
                    button("rps:paper", "Paper"),
                    button("rps:scissors", "Scissors"),
                ],
                id: None,
            })]
        );
        assert_eq!(buttons(&[]), []);
    }

    #[test]
    fn answers_for_everyone_in_the_channel() {
        let data = response(Answer::new("4".to_owned(), Audience::Everyone))
            .data
            .unwrap();
        assert_eq!(data.flags, None);
        assert_eq!(data.allowed_mentions, Some(AllowedMentions::default()));
    }

    #[test]
    fn answers_only_for_the_person_who_asked() {
        let response = response(Answer::new(Text::NotHere.render(Lang::En), Audience::Asker));
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
    fn the_options_of_poll() {
        let commands = commands();
        // /poll, in German /umfrage: the options one must give first.
        let poll = &commands[3];
        assert_eq!(poll.name, "poll");
        let options: Vec<_> = poll
            .options
            .iter()
            .map(|option| (option.name.as_str(), option.required))
            .collect();
        let mut expected = vec![("question", Some(true))];
        let answers: Vec<String> = (1..=10).map(|number| format!("answer{number}")).collect();
        for (index, answer) in answers.iter().enumerate() {
            expected.push((answer.as_str(), Some(index < 2)));
        }
        expected.extend([("hours", None), ("multiple", None)]);
        assert_eq!(options, expected);
        let hours = &poll.options[11];
        assert_eq!(
            (hours.min_value, hours.max_value),
            (Some(Limit::Integer(1)), Some(Limit::Integer(168)))
        );
    }

    #[test]
    fn the_options_of_dice() {
        let commands = commands();
        let dice = &commands[2];
        assert_eq!(dice.name, "dice");
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
    }

    #[test]
    fn the_commands_in_both_languages() {
        let commands = commands();
        let names: Vec<&str> = commands
            .iter()
            .map(|command| command.name.as_str())
            .collect();
        assert_eq!(
            names,
            [
                "info", "settings", "dice", "poll", "server", "user", "coin", "8ball", "rps",
                "guess", SAVE_QUOTE, "quote"
            ]
        );
        for command in &commands {
            assert_eq!(command.contexts, Some(vec![InteractionContextType::Guild]));
        }
        let slash = commands
            .iter()
            .filter(|command| command.kind == CommandType::ChatInput);
        for command in slash {
            let german = &command.description_localizations.as_ref().unwrap()["de"];
            assert_ne!(&command.description, german);
        }
        // In the menu of messages, without a description.
        let save = &commands[10];
        assert_eq!(
            (save.kind, save.description.as_str()),
            (CommandType::Message, "")
        );
        assert_eq!(
            save.name_localizations.as_ref().unwrap()["de"],
            "Zitat speichern"
        );
        // /quote show with a number or without, /quote delete with it.
        let quote = &commands[11];
        let subcommands: Vec<_> = quote
            .options
            .iter()
            .map(|subcommand| {
                let number = &subcommand.options.as_ref().unwrap()[0];
                let german = &subcommand.name_localizations.as_ref().unwrap()["de"];
                (subcommand.name.as_str(), german.as_str(), number.required)
            })
            .collect();
        assert_eq!(
            subcommands,
            [
                ("show", "zeigen", Some(false)),
                ("delete", "löschen", Some(true))
            ]
        );
        let (info, settings) = (&commands[0], &commands[1]);
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

    /// The message of the tests, as Discord sends it with Save quote.
    const MESSAGE: &str = r#"{"messages": {"1300000000000000020": {"id": "1300000000000000020",
        "channel_id": "1300000000000000004", "content": "Wuff!", "type": 0, "flags": 0,
        "author": {"id": "1100000000000000009", "username": "rex", "global_name": "Rex",
            "discriminator": "0", "avatar": null},
        "timestamp": "2026-11-08T12:00:00.000000+00:00", "edited_timestamp": null,
        "tts": false, "mention_everyone": false, "mentions": [], "mention_roles": [],
        "attachments": [], "embeds": [], "components": [], "pinned": false}}}"#;

    /// Save quote, in the menu of the message 1300000000000000020, with what Discord sends of
    /// the messages, as JSON.
    fn saving(messages: &str) -> Interaction {
        let mut interaction = resolved(SAVE_QUOTE, "[]", messages, "de", 0);
        if let Some(InteractionData::ApplicationCommand(command)) = &mut interaction.data {
            command.kind = CommandType::Message;
            command.target_id = Some(Id::new(1_300_000_000_000_000_020));
        }
        interaction
    }

    #[test]
    fn saving_quotes() {
        let guild = Id::new(GUILD);
        let quote = Quote {
            author: 1_100_000_000_000_000_009,
            name: "Rex".to_owned(),
            channel: 1_300_000_000_000_000_004,
            message: 1_300_000_000_000_000_020,
            text: "Wuff!".to_owned(),
        };
        let interaction = saving(MESSAGE);
        assert_eq!(asked(&interaction).0, Request::SaveQuote(guild, quote));
        // A message that Discord didn't send counts as one without text.
        let interaction = saving("null");
        assert_eq!(
            asked(&interaction).0,
            Request::SaveQuote(guild, Quote::default())
        );
    }

    #[test]
    fn showing_and_deleting_quotes() {
        let guild = Id::new(GUILD);
        let subcommand = |name: &str, options: &str| {
            format!(r#"[{{"name": "{name}", "type": 1, "options": {options}}}]"#)
        };
        let number = r#"[{"name": "number", "type": 4, "value": 3}]"#;
        let interaction = command("quote", &subcommand("show", "[]"), "de", 0);
        assert_eq!(asked(&interaction).0, Request::ShowQuote(guild, None));
        let interaction = command("quote", &subcommand("show", number), "de", 0);
        assert_eq!(asked(&interaction).0, Request::ShowQuote(guild, Some(3)));
        // Whether the person who asked may delete the quotes of others.
        let delete = subcommand("delete", number);
        let manage_messages = Permissions::MANAGE_MESSAGES.bits();
        for (permissions, may_manage) in [(0, false), (manage_messages, true)] {
            let interaction = command("quote", &delete, "de", permissions);
            assert_eq!(
                asked(&interaction).0,
                Request::DeleteQuote {
                    guild,
                    number: 3,
                    asker: 1_400_000_000_000_000_005,
                    may_manage
                }
            );
        }
        // Other shapes, of other versions of the command.
        for options in [
            "[]".to_owned(),
            subcommand("delete", "[]"),
            subcommand("edit", "[]"),
        ] {
            let interaction = command("quote", &options, "de", 0);
            assert_eq!(
                asked(&interaction).0,
                Request::Answer(Text::UnknownCommand),
                "{options}"
            );
        }
    }

    /// As Discord takes them, which the fake Discord of the tests doesn't check: names,
    /// descriptions, their translations and the options within Discord's rules.
    #[test]
    fn discord_takes_the_commands() {
        for command in commands() {
            twilight_validate::command::command(&command).unwrap();
            twilight_validate::command::options(&command.options).unwrap();
        }
    }
}
