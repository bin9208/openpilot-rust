use crate::Value;

pub struct Command {
    pub name: &'static str,
    pub summary: &'static str,
    pub usage: &'static str,
    pub hidden: bool,
}
pub const COMMANDS: [Command; 7] = [
    Command {
        name: "help",
        summary: "사용 가능한 Carrot 명령과 사용법을 보여줍니다.",
        usage: "carrot help [command]",
        hidden: false,
    },
    Command {
        name: "vision",
        summary: "주차 상태에서 당근 비전 카메라를 확인합니다.",
        usage: "carrot vision <start|status|logs|stop> [--lines N]",
        hidden: false,
    },
    Command {
        name: "vision_off",
        summary: "Legacy alias for carrot vision stop.",
        usage: "carrot vision stop",
        hidden: true,
    },
    Command {
        name: "vision_on",
        summary: "Legacy alias for carrot vision start.",
        usage: "carrot vision start",
        hidden: true,
    },
    Command {
        name: "web-intro",
        summary: "실제 설치 인트로를 저장 없는 미리보기로 엽니다.",
        usage: "carrot web-intro",
        hidden: false,
    },
    Command {
        name: "web-lab",
        summary: "Carrot Web 실험 기능의 잠금을 관리합니다.",
        usage: "carrot web-lab <on|off|status>",
        hidden: false,
    },
    Command {
        name: "youtube-test",
        summary: "Run the YouTube camera pipeline while the device is offroad.",
        usage: "carrot youtube-test [verify|start|status|logs|stop] [--lines N]",
        hidden: false,
    },
];
pub fn get(name: &str) -> Option<&'static Command> {
    let name = name.trim().to_lowercase();
    COMMANDS.iter().find(|command| command.name == name)
}
pub fn listing() -> Value {
    Value::object([
        ("ok", Value::Bool(true)),
        ("command", Value::text("carrot")),
        (
            "commands",
            Value::Array(
                COMMANDS
                    .iter()
                    .filter(|command| !command.hidden)
                    .map(|command| {
                        Value::object([
                            ("name", Value::text(command.name)),
                            ("summary", Value::text(command.summary)),
                            ("usage", Value::text(command.usage)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}
