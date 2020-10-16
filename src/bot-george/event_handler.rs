use crate::{commands, db::DbPool, error::Result};
use dispose::defer;
use lazy_static::lazy_static;
use log::*;
use regex::Regex;
use serenity::{
    async_trait,
    client::{Context, EventHandler},
    model::{
        channel::{Channel, Message},
        gateway::{Activity, Ready},
        id::GuildId,
        user::OnlineStatus,
    },
};
use std::{
    fmt::{Display, Write},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    thread,
};
use tokio::runtime;

// TODO: this is here because async closures are unstable
macro_rules! stupid_try {
    ($x:expr) => {
        match $x {
            Ok(o) => o,
            Err(e) => {
                error!("{}", e);
                return;
            },
        }
    };
}

lazy_static! {
    static ref WORD_FINI_RE: Regex = Regex::new(r"\w$").unwrap();
}

pub struct Handler {
    prefix: String,
    prefix_re: Regex,
    superuser: u64,
    pool: DbPool,
    me: AtomicU64,
}

impl Handler {
    pub fn new(prefix: impl AsRef<str>, superuser: u64, pool: DbPool) -> Result<Self> {
        let prefix_re = Regex::new(&format!(
            r"^\s*{}{}",
            regex::escape(prefix.as_ref()),
            if WORD_FINI_RE.is_match(prefix.as_ref()) {
                r"\b"
            } else {
                ""
            }
        ))?;

        return Ok(Self {
            prefix: prefix.as_ref().into(),
            prefix_re,
            superuser,
            pool,
            me: 0.into(),
        });
    }

    pub fn prefix_command<C: Display>(&self, command: C) -> String {
        let mut ret = String::new();

        write!(ret, "{}{}", self.prefix, command).unwrap();

        if !self.prefix_re.is_match(&ret) {
            ret.clear();

            write!(ret, "{} {}", self.prefix, command).unwrap();

            // If neither of these work then we're in trouble
            assert!(self.prefix_re.is_match(&ret));
        }

        ret
    }
}

impl Handler {
    async fn handle_command<S: AsRef<str>>(&self, s: S, ctx: Context, msg: &Message) {
        let cid = msg.channel_id;
        let http = Arc::clone(&ctx.http);

        let _d = defer(|| {
            if thread::panicking() {
                tokio::task::spawn_blocking(move || {
                    runtime::Builder::new()
                        .enable_all()
                        .basic_scheduler()
                        .max_threads(1)
                        .build()
                        .unwrap()
                        .block_on(cid.send_message(http, |m| {
                            m.content("**ERROR:** thread panicked while servicing your request")
                        }))
                        .ok()
                });
            }
        });

        let cmd = match commands::parse_base(s) {
            Ok(c) => c,
            Err(e) => {
                stupid_try!(
                    msg.channel_id
                        .send_message(&ctx, |m| m.content(format!("**```{:#?}```**", e)))
                        .await
                );
                return;
            },
        };

        use crate::commands::BaseCommand::*;

        stupid_try!(
            msg.channel_id
                .send_message(&ctx, |m| m.content(format!("```{:#?}```", cmd)))
                .await
        );

        match cmd {
            Help(_topic) => todo!(),
            Role(rcmd) => {
                use crate::commands::RoleCommand::*;
                match rcmd {
                    Help(_topic) => todo!(),
                    List => todo!(),
                    Show(_user) => todo!(),
                    Add(_user, _roles) => todo!(),
                    Remove(_user, _roles) => todo!(),
                }
            },
            Modmail(_message) => todo!(),
        }
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        self.me.store(*ready.user.id.as_u64(), Ordering::Release);

        ctx.set_presence(
            Some(Activity::playing(&format!(
                "CS:GO | {}",
                self.prefix_command("help")
            ))),
            OnlineStatus::Online,
        )
        .await;
    }

    async fn message(&self, ctx: Context, msg: Message) {
        let me = self.me.load(Ordering::Acquire);

        // TODO TODO TODO TODO TODO TODO
        match msg.guild_id {
            None | Some(GuildId(145351184943808512)) => (),
            _ => {
                error!("Cowardly refusing to aoid spamming servers!");
                return;
            },
        }

        if *msg.author.id.as_u64() == me {
            return;
        }

        if msg.author.bot {
            info!("Ignoring message from bot {:?}", msg.author);
            return;
        }

        if let Some(mat) = self.prefix_re.find(&msg.content) {
            let rest = &msg.content[mat.end()..];

            self.handle_command(rest, ctx, &msg).await;
        } else if let Channel::Private(..) = stupid_try!(msg.channel_id.to_channel(&ctx).await) {
            self.handle_command(&msg.content, ctx, &msg).await;
        }
        // TODO: identify if the message is Important(tm)
    }
}
