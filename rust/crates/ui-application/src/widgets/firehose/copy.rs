//! Original localized Firehose settings copy (MIT), shared by both native layouts.
pub const TITLE: &str = "Firehose Mode";
pub const DESCRIPTION: &str = "openpilot learns to drive by watching humans, like you, drive.\n\nFirehose Mode allows you to maximize your training data uploads to improve openpilot's driving models. More data means bigger models, which means better Experimental Mode.";
pub const INSTRUCTIONS_INTRO: &str = "For maximum effectiveness, bring your device inside and connect to a good USB-C adapter and Wi-Fi weekly.\n\nFirehose Mode can also work while you're driving if connected to a hotspot or unlimited SIM card.";
pub const FAQ_HEADER: &str = "Frequently Asked Questions";
pub const FAQ_ITEMS: &[(&str, &str)] = &[
    (
        "Does it matter how or where I drive?",
        "Nope, just drive as you normally would.",
    ),
    (
        "Do all of my segments get pulled in Firehose Mode?",
        "No, we selectively pull a subset of your segments.",
    ),
    (
        "What's a good USB-C adapter?",
        "Any fast phone or laptop charger should be fine.",
    ),
    (
        "Does it matter which software I run?",
        "Yes, only upstream openpilot (and particular forks) are able to be used for training.",
    ),
];
pub const INSTRUCTIONS: &str = "For maximum effectiveness, bring your device inside and connect to a good USB-C adapter and Wi-Fi weekly.\n\nFirehose Mode can also work while you're driving if connected to a hotspot or unlimited SIM card.\n\n\nFrequently Asked Questions\n\nDoes it matter how or where I drive? Nope, just drive as you normally would.\n\nDo all of my segments get pulled in Firehose Mode? No, we selectively pull a subset of your segments.\n\nWhat's a good USB-C adapter? Any fast phone or laptop charger should be fine.\n\nDoes it matter which software I run? Yes, only upstream openpilot (and particular forks) are able to be used for training.";
