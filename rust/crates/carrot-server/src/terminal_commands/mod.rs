//! Seven active handlers and CLI bridge from terminal_commands/{cli,registry,custom_commands}.
mod arguments;
pub mod registry;
use crate::{vision_test, web_settings::WebSettings, youtube_test, Error};

pub struct Config {
    pub web: WebSettings,
    pub vision: Option<vision_test::Config>,
    pub youtube: Option<youtube_test::Config>,
}
impl Config {
    pub fn original() -> Result<Self, Error> {
        let root = crate::config::runtime_repository()?;
        let config = crate::config::Config::from_environment(&root);
        Ok(Self {
            web: WebSettings::new(
                &config.state.join("web_settings.json"),
                &config
                    .web
                    .join("src/features/drive/core/content_catalog.json"),
            ),
            vision: None,
            youtube: None,
        })
    }
}
pub async fn run(config: &Config, argv: &[String]) -> Result<i32, Error> {
    let parts = match arguments::parse(argv) {
        Ok(parts) => parts,
        Err(message) => {
            arguments::error(&message);
            return Ok(2);
        }
    };
    let name = parts.first().map_or("help", String::as_str);
    let args = if parts.is_empty() {
        &[][..]
    } else {
        &parts[1..]
    };
    let Some(command) = registry::get(name) else {
        eprintln!("[terminal] unknown command: {name}\n[terminal] run 'carrot help' to list available commands");
        return Ok(2);
    };
    let result = handler(config, command.name, args).await;
    match result {
        Ok(code) => Ok(code),
        Err(error) => {
            eprintln!("[terminal] {} failed: {error}", command.name);
            Ok(1)
        }
    }
}
fn parse_error(line: &str) -> &'static str {
    // Only recover the existing shlex parser's two error labels; tokenization
    // remains the already-verified Tools implementation.
    let mut quote = None;
    let mut escaped = false;
    for c in line.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' && quote != Some('\'') {
            escaped = true;
        } else if quote == Some(c) {
            quote = None;
        } else if quote.is_none() && matches!(c, '\'' | '"') {
            quote = Some(c);
        }
    }
    if escaped {
        "No escaped character"
    } else {
        "No closing quotation"
    }
}
async fn handler(config: &Config, name: &str, args: &[String]) -> Result<i32, Error> {
    match name {
        "help" => {
            if let Some(name) = args.first() {
                let Some(command) = registry::get(name) else {
                    println!("[terminal] unknown command: {name}");
                    return Ok(2);
                };
                println!("{}\n  {}", command.usage, command.summary);
            } else {
                println!("Carrot 명령");
                for command in registry::COMMANDS.iter().filter(|command| !command.hidden) {
                    println!("  carrot {:<14} {}", command.name, command.summary);
                }
                println!("\n자세한 사용법: carrot help <명령>");
            }
            Ok(0)
        }
        "web-intro" => {
            if !args.is_empty() {
                println!("사용법: carrot web-intro");
                return Ok(2);
            }
            println!("[web-intro] 실제 인트로를 미리보기로 엽니다. 변경사항은 저장되지 않습니다.\n[[CARROT_WEB_ACTION:web-intro]]");
            Ok(0)
        }
        "web-lab" => {
            match args
                .first()
                .map_or("status", String::as_str)
                .trim()
                .to_lowercase()
                .as_str()
            {
                "status" | "-s" => println!(
                    "[web-lab] {}",
                    if config.web.read()?.get("web_lab_enabled").truth() {
                        "on"
                    } else {
                        "off"
                    }
                ),
                "on" | "enable" => {
                    config.web.set_capability("web_lab", true)?;
                    println!("[web-lab] on - 실험 기능 잠금이 해제되었습니다.");
                }
                "off" | "disable" => {
                    config.web.set_capability("web_lab", false)?;
                    println!("[web-lab] off - 실험 기능을 끄고 잠갔습니다.");
                }
                _ => {
                    println!("사용법: carrot web-lab <on|off|status>");
                    return Ok(2);
                }
            }
            Ok(0)
        }
        "vision" | "vision_on" | "vision_off" => {
            let args = if name == "vision" {
                let Some(first) = args.first() else {
                    vision_help();
                    return Ok(0);
                };
                let action = first.to_lowercase();
                if matches!(action.as_str(), "help" | "-h" | "--help") {
                    vision_help();
                    return Ok(0);
                }
                let action = match action.as_str() {
                    "on" => "start",
                    "off" => "stop",
                    "log" => "logs",
                    _ => &action,
                };
                std::iter::once(action.to_owned())
                    .chain(args[1..].iter().cloned())
                    .collect::<Vec<_>>()
            } else {
                std::iter::once(if name == "vision_on" { "start" } else { "stop" }.into())
                    .chain(args.iter().cloned())
                    .collect()
            };
            let owned;
            let vision = if let Some(vision) = &config.vision {
                vision
            } else {
                owned = vision_test::Config::original()?;
                &owned
            };
            vision_test::run_command(vision, &args).await
        }
        "youtube-test" => {
            if youtube_test::help(args) {
                return Ok(0);
            }
            let owned;
            let youtube = if let Some(youtube) = &config.youtube {
                youtube
            } else {
                owned = youtube_test::Config::original()?;
                &owned
            };
            youtube_test::run_command(youtube, args).await
        }
        _ => Err(Error::Source("unregistered terminal command".into())),
    }
}
fn vision_help() {
    println!("사용법: carrot vision <start|status|logs|stop> [--lines N]\n  start   카메라 확인 시작\n  status  현재 준비 상태 확인\n  logs    최근 확인 로그 표시\n  stop    카메라 확인 중지");
}
