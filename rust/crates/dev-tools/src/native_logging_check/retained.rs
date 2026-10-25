//! Native-only consumer of the independently admitted, inert R11 C++ records.
//! The live retained mode accepts no archive, builder, executable reference or
//! source hook. Its private prepare child only admits pinned inert archive data.
use super::*;
use serde::{
    de::{MapAccess, Visitor},
    Deserialize, Deserializer,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fmt,
    os::{
        fd::AsRawFd,
        unix::fs::{FileExt, MetadataExt},
    },
    path::Component,
};

const CAPTURE_SOURCE: &str = "300f77e423750804b2a781d734128945a4b56765";
const CAPTURE_ROOT: &str = "/tmp/lgctx-39888221765-1.2h2aAH9y/source";
const CAPTURE_BINARY: &str =
    "/tmp/lgctx-39888221765-1.2h2aAH9y/reference-held/native_logging_reference";
const CAPTURE_ELF: &str = "2a7e6e8e4a6450de1edc9c97ebb834e2c5054f0a27601fca93b83107039fff10";
const OLD_SOURCE: &str = "cdc7f7c822736e913a672d15227a8e8f0cd0199d";
const OLD_ROOT: &str = "/home/runner/work/openpilot-rust/openpilot-rust";
const OLD_BINARY: &str =
    "/home/runner/work/_temp/native-logging-reference-r1/native_logging_reference";
const OLD_ELF: &str = "d540c13737a12ff46215fade79aa81ec705b7fe4ab37297890cde9f5d3a0d4b6";
const RECOVERY_ZIP: &str = "05ff6420f05f8b51f457b85ecf34ad514bc75b6b6147e4abf5d8f78d7f85462f";
const PROOF_GZIP: &str = "2dfea2b755d8267132c81a240a84d1fb4f653cc4373f3d2328bc11891f7b1879";
const PROBE: &str = "rust/crates/logging/examples/native_logging_probe.rs";
const CPP_SITE: &str = "rust/tools/native_logging_reference.cc";
const SOURCE_LEAVES: [&str; 2] = [
    "rust/tools/check_native_logging.py",
    "rust/tools/native_logging_transport.py",
];
// This is an import/source scope, not a claim of complete Cargo/DSO closure.
const SOURCE_FILES: &[&str] = &[
    "rust/Cargo.toml",
    "rust/Cargo.lock",
    "rust/crates/dev-tools/Cargo.toml",
    "rust/crates/dev-tools/src/lib.rs",
    "rust/crates/dev-tools/src/bin/check_native_logging.rs",
    "rust/crates/dev-tools/src/native_logging_check.rs",
    "rust/crates/dev-tools/src/native_logging_check/retained.rs",
    "rust/crates/dev-tools/src/native_logging_check/retained_tests.rs",
    "rust/crates/dev-tools/src/native_logging_check/lifecycle.rs",
    "rust/crates/dev-tools/src/native_logging_check/socket.rs",
    "rust/crates/dev-tools/src/native_logging_check/transport.rs",
    "rust/crates/dev-tools/src/argparse_scan.rs",
    "rust/crates/dev-tools/src/qa_process.rs",
    "rust/crates/dev-tools/src/seal.rs",
    "rust/crates/logging/Cargo.toml",
    "rust/crates/logging/build.rs",
    "rust/crates/logging/src/native.rs",
    "rust/crates/logging/src/native/wire.rs",
    PROBE,
];

// Exact imported member hashes; captured data stays private and uncommitted.
const MEMBERS: &[(&str, u64, &str)] = &[
    (
        "historical/api.json",
        722,
        "5fc2970811b13553a6758261c8c9bf2a03c49df846d904170ba4b4169ed20c61",
    ),
    (
        "historical/console-0/invocation.json",
        435,
        "45ac7590ea6358d280fa3fe17d3e067235a5f578081be0db3af76fd503eac7a4",
    ),
    (
        "historical/console-0/packets.json",
        18426,
        "c321cabfef08d86b7efc9440e4e241967fc96e576181b1b1aa121bf8cb6888b6",
    ),
    (
        "historical/console-0/responses.json",
        5137,
        "1b43f6ae477853eaa78315e847963c1ef0dc40b1caaa6a5219888fecdb4aecb4",
    ),
    (
        "historical/console-0/responses.jsonl",
        3035,
        "19e8716a3b9ffe0a3434ed40c824e58f28a68f28ede0ccb345f59a9955791cce",
    ),
    (
        "historical/console-0/stdout.txt",
        1179,
        "2642857fa83fcb0279194d2ba32cfbc08de6b2d08ed934487238da6a2645c42e",
    ),
    (
        "historical/console-1/invocation.json",
        436,
        "057bf22269b57f5e5c00746e6c666c49575891bcdf0274a8f2df17e5a7eabb90",
    ),
    (
        "historical/console-1/packets.json",
        26588,
        "68d249b9d8b218d801f564ff663eba56ce55ba5a4f72454648d2b56ab2b8dc1c",
    ),
    (
        "historical/console-1/responses.json",
        5137,
        "1b43f6ae477853eaa78315e847963c1ef0dc40b1caaa6a5219888fecdb4aecb4",
    ),
    (
        "historical/console-1/responses.jsonl",
        3035,
        "19e8716a3b9ffe0a3434ed40c824e58f28a68f28ede0ccb345f59a9955791cce",
    ),
    (
        "historical/console-1/stdout.txt",
        1179,
        "2642857fa83fcb0279194d2ba32cfbc08de6b2d08ed934487238da6a2645c42e",
    ),
    (
        "historical/console-2/invocation.json",
        435,
        "52d96c20bacbf7198f5cf733ef52b8764af14c4efc78697adfca6956e8262c67",
    ),
    (
        "historical/console-2/packets.json",
        18489,
        "c6b30fef38e034fffb8c29ede897878a5e49e9d88cd2c775db76a28ccd36b152",
    ),
    (
        "historical/console-2/responses.json",
        5137,
        "1b43f6ae477853eaa78315e847963c1ef0dc40b1caaa6a5219888fecdb4aecb4",
    ),
    (
        "historical/console-2/responses.jsonl",
        3035,
        "19e8716a3b9ffe0a3434ed40c824e58f28a68f28ede0ccb345f59a9955791cce",
    ),
    (
        "historical/console-2/stdout.txt",
        1965,
        "9e26c44c06ea1c098a36ce923f52a6c1e69f891ada9e81ed065f9f01a99e914b",
    ),
    (
        "historical/console-3/invocation.json",
        436,
        "63a002f3cc06e62d63614dd00d2951dd655e3f441d2abd37bec26a76691412ad",
    ),
    (
        "historical/console-3/packets.json",
        26604,
        "1e29cb6157698a8f5dd3bdcb960f245bf75ca56263a288b862c1b69f803bba06",
    ),
    (
        "historical/console-3/responses.json",
        5137,
        "1b43f6ae477853eaa78315e847963c1ef0dc40b1caaa6a5219888fecdb4aecb4",
    ),
    (
        "historical/console-3/responses.jsonl",
        3035,
        "19e8716a3b9ffe0a3434ed40c824e58f28a68f28ede0ccb345f59a9955791cce",
    ),
    (
        "historical/console-3/stdout.txt",
        1572,
        "672313ed072888850fe3219a59db194bb2972f7af7b1903e84ebaf98eb115c71",
    ),
    (
        "historical/console-4/invocation.json",
        436,
        "506f1f331f83a696e8f1252491ad0b4a96e388b92511547a8787b292275bbfdd",
    ),
    (
        "historical/console-4/packets.json",
        18422,
        "b05edc508c08b11de59ec3fc29e03b24df7248c6bca6a75cea12780456795018",
    ),
    (
        "historical/console-4/responses.json",
        5137,
        "1b43f6ae477853eaa78315e847963c1ef0dc40b1caaa6a5219888fecdb4aecb4",
    ),
    (
        "historical/console-4/responses.jsonl",
        3035,
        "19e8716a3b9ffe0a3434ed40c824e58f28a68f28ede0ccb345f59a9955791cce",
    ),
    (
        "historical/console-4/stdout.txt",
        1179,
        "2642857fa83fcb0279194d2ba32cfbc08de6b2d08ed934487238da6a2645c42e",
    ),
    (
        "historical/console-5/invocation.json",
        436,
        "bc4cec5538fa026f15757be99d3818335a64396ac8910ad912a11eb9601092af",
    ),
    (
        "historical/console-5/packets.json",
        26601,
        "f56c20cc619d43c36d92a34d4bb86a8329762d2cbc0974edddf2674963d5b6f6",
    ),
    (
        "historical/console-5/responses.json",
        5137,
        "1b43f6ae477853eaa78315e847963c1ef0dc40b1caaa6a5219888fecdb4aecb4",
    ),
    (
        "historical/console-5/responses.jsonl",
        3035,
        "19e8716a3b9ffe0a3434ed40c824e58f28a68f28ede0ccb345f59a9955791cce",
    ),
    (
        "historical/console-5/stdout.txt",
        1179,
        "2642857fa83fcb0279194d2ba32cfbc08de6b2d08ed934487238da6a2645c42e",
    ),
    (
        "historical/console-6/invocation.json",
        436,
        "ac3b9bf0ea02c17efd21284381cba69dcce346369fdfdec20769854deef093a6",
    ),
    (
        "historical/console-6/packets.json",
        18420,
        "23436c199116645d89695c0867a48aa9289e91f89e7e47e3c8fde39dd8af522d",
    ),
    (
        "historical/console-6/responses.json",
        5137,
        "1b43f6ae477853eaa78315e847963c1ef0dc40b1caaa6a5219888fecdb4aecb4",
    ),
    (
        "historical/console-6/responses.jsonl",
        3035,
        "19e8716a3b9ffe0a3434ed40c824e58f28a68f28ede0ccb345f59a9955791cce",
    ),
    (
        "historical/console-6/stdout.txt",
        1179,
        "2642857fa83fcb0279194d2ba32cfbc08de6b2d08ed934487238da6a2645c42e",
    ),
    (
        "historical/providers.after.json",
        754,
        "c379c927514362a71df8e6a7bae48b46c1512cc16d0248a32a497ee7fe88349b",
    ),
    (
        "historical/providers.before.json",
        754,
        "c379c927514362a71df8e6a7bae48b46c1512cc16d0248a32a497ee7fe88349b",
    ),
    (
        "recovered/capture-complete.json",
        580,
        "14c26b5537fde002caee552b0ea1da207e914b9b374696ec552078aa7e4ebcc1",
    ),
    (
        "recovered/console-0/invocation.json",
        444,
        "247fb1a569b18c3edf1c991d6f3f42a80733a1ca7e6f9c298f28632dff1d37e8",
    ),
    (
        "recovered/console-0/packets.json",
        18020,
        "68d5c2cd5b402aad850cba6a0d944c831a40fbc8598ef6869f680be7daddeea1",
    ),
    (
        "recovered/console-0/responses.json",
        5137,
        "1b43f6ae477853eaa78315e847963c1ef0dc40b1caaa6a5219888fecdb4aecb4",
    ),
    (
        "recovered/console-0/responses.jsonl",
        3035,
        "19e8716a3b9ffe0a3434ed40c824e58f28a68f28ede0ccb345f59a9955791cce",
    ),
    (
        "recovered/console-0/stdout.txt",
        1095,
        "6c442ddf470071716f9550bd4ae3164760ce1e2fe84ffe9653aad9f7ab2f9b9d",
    ),
    (
        "recovered/console-1/invocation.json",
        444,
        "f9fba9562f7134610a68b4ea86208037e534c140d0f9051cab9b78701aacc1c6",
    ),
    (
        "recovered/console-1/packets.json",
        26200,
        "59ba2254f13250acb62134313f25457df2228af15e16e526424f39cb23b420f1",
    ),
    (
        "recovered/console-1/responses.json",
        5137,
        "1b43f6ae477853eaa78315e847963c1ef0dc40b1caaa6a5219888fecdb4aecb4",
    ),
    (
        "recovered/console-1/responses.jsonl",
        3035,
        "19e8716a3b9ffe0a3434ed40c824e58f28a68f28ede0ccb345f59a9955791cce",
    ),
    (
        "recovered/console-1/stdout.txt",
        1095,
        "6c442ddf470071716f9550bd4ae3164760ce1e2fe84ffe9653aad9f7ab2f9b9d",
    ),
    (
        "recovered/console-2/invocation.json",
        444,
        "3808ee2b595f325f973653a9d605d66f9854c81f22df834487b4da94000c5ad5",
    ),
    (
        "recovered/console-2/packets.json",
        18082,
        "1df941afb3ef8ca1d0bdbfb3687ae3907b2bae6b2bcbe9daafdba0d7ecc3b83d",
    ),
    (
        "recovered/console-2/responses.json",
        5137,
        "1b43f6ae477853eaa78315e847963c1ef0dc40b1caaa6a5219888fecdb4aecb4",
    ),
    (
        "recovered/console-2/responses.jsonl",
        3035,
        "19e8716a3b9ffe0a3434ed40c824e58f28a68f28ede0ccb345f59a9955791cce",
    ),
    (
        "recovered/console-2/stdout.txt",
        1825,
        "49decd624ca7aeed2e1c48163861b2457dd372446b30bbfcaead236d5a7b51b6",
    ),
    (
        "recovered/console-3/invocation.json",
        443,
        "d89ddcc88c1f89aef004e91e818b92037ca09306b9385ce7c6d1e2232a96077d",
    ),
    (
        "recovered/console-3/packets.json",
        26202,
        "01b48a496148f45be102abee07429a2e0b1206afee9ab52897a80a76dc9e8718",
    ),
    (
        "recovered/console-3/responses.json",
        5137,
        "1b43f6ae477853eaa78315e847963c1ef0dc40b1caaa6a5219888fecdb4aecb4",
    ),
    (
        "recovered/console-3/responses.jsonl",
        3035,
        "19e8716a3b9ffe0a3434ed40c824e58f28a68f28ede0ccb345f59a9955791cce",
    ),
    (
        "recovered/console-3/stdout.txt",
        1460,
        "33ec24dc145dab323a52b4cfa89710e356477492bcce44b945605742c408795c",
    ),
    (
        "recovered/console-4/invocation.json",
        444,
        "e951a9bd59ab92ad8de6be0e1bd3aa528c121d8afc436c7a994e13668b6e3a79",
    ),
    (
        "recovered/console-4/packets.json",
        18023,
        "9f239fb308d3a731479e9b73fc7ddbb14e382e693e8757123463db2a28ec1d8c",
    ),
    (
        "recovered/console-4/responses.json",
        5137,
        "1b43f6ae477853eaa78315e847963c1ef0dc40b1caaa6a5219888fecdb4aecb4",
    ),
    (
        "recovered/console-4/responses.jsonl",
        3035,
        "19e8716a3b9ffe0a3434ed40c824e58f28a68f28ede0ccb345f59a9955791cce",
    ),
    (
        "recovered/console-4/stdout.txt",
        1095,
        "6c442ddf470071716f9550bd4ae3164760ce1e2fe84ffe9653aad9f7ab2f9b9d",
    ),
    (
        "recovered/console-5/invocation.json",
        443,
        "089a0d08dc5e812b29855d91a22a437454d36b6a8fccade5bfeb3474a94ddb94",
    ),
    (
        "recovered/console-5/packets.json",
        26201,
        "8711ef6354cbdd4b617b8d81150d9d49e1919ee39fca54c893a20ac9ab2a60e8",
    ),
    (
        "recovered/console-5/responses.json",
        5137,
        "1b43f6ae477853eaa78315e847963c1ef0dc40b1caaa6a5219888fecdb4aecb4",
    ),
    (
        "recovered/console-5/responses.jsonl",
        3035,
        "19e8716a3b9ffe0a3434ed40c824e58f28a68f28ede0ccb345f59a9955791cce",
    ),
    (
        "recovered/console-5/stdout.txt",
        1095,
        "6c442ddf470071716f9550bd4ae3164760ce1e2fe84ffe9653aad9f7ab2f9b9d",
    ),
    (
        "recovered/console-6/invocation.json",
        444,
        "273c31deaf6ff975ba3c1a55b6e1173b918574135a3a11e1559294e067ebb181",
    ),
    (
        "recovered/console-6/packets.json",
        18021,
        "b25b6241500091c85a42a8265adb4c20886bab649f970a9cec1de6190dc7750b",
    ),
    (
        "recovered/console-6/responses.json",
        5137,
        "1b43f6ae477853eaa78315e847963c1ef0dc40b1caaa6a5219888fecdb4aecb4",
    ),
    (
        "recovered/console-6/responses.jsonl",
        3035,
        "19e8716a3b9ffe0a3434ed40c824e58f28a68f28ede0ccb345f59a9955791cce",
    ),
    (
        "recovered/console-6/stdout.txt",
        1095,
        "6c442ddf470071716f9550bd4ae3164760ce1e2fe84ffe9653aad9f7ab2f9b9d",
    ),
    (
        "recovered/protocol-result.json",
        1798,
        "9f6d743ea40b84396a4f0e6248db053eaf0d94dbb93361cd7f24391ca0c47270",
    ),
    (
        "recovered/providers.after.json",
        734,
        "48458276cd87e1f4ef5e93ef87172698682c9c7769a4d8c6c1796439e06737c1",
    ),
    (
        "recovered/providers.before.json",
        734,
        "48458276cd87e1f4ef5e93ef87172698682c9c7769a4d8c6c1796439e06737c1",
    ),
    (
        "recovered/rate/invocation.json",
        444,
        "8286a4fdb8f1941fe6ee2db4a4c66a70ebd719ecfe891dcc89f2b69105727690",
    ),
    (
        "recovered/rate/packets.json",
        12220,
        "4a948dad5a51c2fe2bb26131759313c5d0f2dc9f307b3f98e305f20940fc084f",
    ),
    (
        "recovered/rate/responses.json",
        111422,
        "4348eabfa77ad185dd137249566104c5ed6d0f671391c120ea09d81a3fec983e",
    ),
    (
        "recovered/rate/responses.jsonl",
        68076,
        "a726634af304357dc9fa21427043c3b73ce4180ff3d4e06d4b1dd52f19786c07",
    ),
    (
        "recovered/rate/stdout.txt",
        454,
        "a8fb24272d4db89eeb7b01c47e411107ff44241f476e71d382ce88fcd09ac86c",
    ),
    (
        "recovered/reference-identity.json",
        1491,
        "13bc8198b3bd3a6e2a57dbdfd3719e91a9d3ae701fd74f554af14d317199b8f1",
    ),
    (
        "recovered/reference-provenance.json",
        1743,
        "64b27129dc1574d8b5cbf9aab39274cc8c21756e7df2169d0a511dd86945e053",
    ),
    (
        "recovered/source-state.after.json",
        518,
        "c86938cea088a1ee02b6e0dd9658efc447aacffec2714d8582a484d9239aa300",
    ),
    (
        "recovered/source-state.before.json",
        518,
        "c86938cea088a1ee02b6e0dd9658efc447aacffec2714d8582a484d9239aa300",
    ),
    (
        "recovered/transport/backpressure/invocation.json",
        452,
        "73b1675a508eb287d127145d50ab5f96ab9db6ef2d2fd0cd12d49f6e707c41dc",
    ),
    (
        "recovered/transport/backpressure/packets.json",
        2,
        "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945",
    ),
    (
        "recovered/transport/backpressure/responses.json",
        182,
        "3430876e4c4cd914a4b99cfdcd57a2c829240d3f92299f71df082a87d761c17f",
    ),
    (
        "recovered/transport/backpressure/responses.jsonl",
        104,
        "a1e69aab2d97e7fc452f046f4505a9358604881e1dd144f69cc574ba5f8463f6",
    ),
    (
        "recovered/transport/backpressure/stdout.txt",
        0,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    ),
    (
        "recovered/transport/backpressure/timing.json",
        282,
        "25fd2ecf96c3e549596a0dc92c7564738a59a1a7e5f43bd55eb67860c18ab30f",
    ),
    (
        "recovered/transport/console-failure/invocation.json",
        444,
        "09f974a5f5b5d68b6e37e6aa08da63b3408c801e21511104765fef96198735ba",
    ),
    (
        "recovered/transport/console-failure/packets.json",
        797,
        "0923855aeb03c542c37f8371bb7c41bbe2bf5cd007b1a28d9794f0e954ebe04c",
    ),
    (
        "recovered/transport/console-failure/responses.json",
        223,
        "3ced0383554f2348ba55cb084351bab48983221fb4b67ba047d09f99fc57bf96",
    ),
    (
        "recovered/transport/console-failure/responses.jsonl",
        137,
        "fcb9a5569e4964c702338358258ca2a598892d030d750c1cc9301976e887a230",
    ),
    (
        "recovered/transport/empty-invalid-endpoint.json",
        99,
        "26be6145e80dfe79ca71fbf1f40a6655ff46103776d03aa413e928ed061dcda5",
    ),
    (
        "recovered/transport/nonempty-invalid-endpoint.json",
        100,
        "afab6d90f59254b2db838830e21a8e000383edaa3e48c2d48de157b2dffd703b",
    ),
    (
        "recovered/transport/SIGINT/invocation.json",
        452,
        "f0b6b0e0818712e84f433afc95f2cf0bbb7427feff36f2c4bab59ae18feccfa5",
    ),
    (
        "recovered/transport/SIGINT/signal.json",
        70,
        "6f36670675141db8d1eedff281b1a93c4d3e3ea289d81845764b4ada91c29132",
    ),
    (
        "recovered/transport/SIGTERM/invocation.json",
        452,
        "64d3138b314219277cd8b6ce9b7d62085f14e89c5bac9e1089d1461e73e2f604",
    ),
    (
        "recovered/transport/SIGTERM/signal.json",
        72,
        "34fc146140b310298e0e168ae8614de0860e63d683651bb212b4b272bac76727",
    ),
    (
        "recovered/transport/threads/invocation.json",
        443,
        "7bd7b7216be5a0b652973c66d72d8c271a329dd892afe85692b338e508b96d80",
    ),
    (
        "recovered/transport/threads/packets.json",
        47400,
        "af434556e258709ce2e87b83ac16e8a7c131bb8cc1a61b6eb023039c796c7004",
    ),
    (
        "recovered/transport/threads/responses.json",
        177,
        "f72e1f867437013a07035b08ecc2114d6ce573e54ef120bf4b853ea33a0f2df2",
    ),
    (
        "recovered/transport/threads/responses.jsonl",
        99,
        "3c92cde07fd7c93fd28e4756cf362429c462f308c0b4de442329ef6cc56d7695",
    ),
    (
        "recovered/transport/threads/stdout.txt",
        0,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    ),
    (
        "reference/native_logging_reference.cc",
        2581,
        "f4c2f9ba56f48595865eba8efb88f7b2b49eaae60d91e79da72a8e39edced022",
    ),
];

#[cfg(test)]
fn expected_members() -> &'static [(&'static str, u64, &'static str)] {
    MEMBERS
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pin {
    size: u64,
    sha256: String,
}

fn unique_map<'de, D: Deserializer<'de>>(
    de: D,
) -> std::result::Result<BTreeMap<String, Pin>, D::Error> {
    struct Unique;
    impl<'de> Visitor<'de> for Unique {
        type Value = BTreeMap<String, Pin>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("unique member names")
        }
        fn visit_map<M: MapAccess<'de>>(
            self,
            mut map: M,
        ) -> std::result::Result<Self::Value, M::Error> {
            let mut result = BTreeMap::new();
            while let Some((name, pin)) = map.next_entry::<String, Pin>()? {
                if result.insert(name, pin).is_some() {
                    return Err(serde::de::Error::custom("duplicate member name"));
                }
            }
            Ok(result)
        }
    }
    de.deserialize_map(Unique)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    schema: u32,
    recovery_run: u64,
    recovery_artifact: u64,
    recovery_zip_sha256: String,
    proof_gzip_sha256: String,
    #[serde(deserialize_with = "unique_map")]
    members: BTreeMap<String, Pin>,
    #[serde(deserialize_with = "unique_map")]
    source_files: BTreeMap<String, Pin>,
}
fn parse_index(bytes: &[u8]) -> Result<Index> {
    require(bytes.len() <= 128 * 1024, "retained index exceeds bound")?;
    Ok(serde_json::from_slice(bytes)?)
}
fn hex(text: &str, len: usize) -> bool {
    text.len() == len
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn fresh_provider_pin(sha: &str) -> Result<()> {
    require(hex(sha, 64), "retained native hash invalid")?;
    require(
        ![
            CAPTURE_ELF,
            OLD_ELF,
            "793786e2342e2cece236ab804b998c0e86311fc5de78403c7ff3663ade200c1c",
            "843f50b05c9cb025de1dd074990dbbb2c076a620edf7defe21734665a126f760",
            "837e6e106882bb296d7de69a5c14bd93e3edab22faf51b594e537eaa70275b91",
        ]
        .contains(&sha),
        "retained or historical provider cannot be executed",
    )
}
fn member_name(name: &str) -> Result<()> {
    require(
        !name.is_empty()
            && !name.starts_with('/')
            && !name.chars().any(|c| matches!(c, '\\' | '\0' | '\n' | '\r'))
            && name
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != ".."),
        "invalid retained member path",
    )
}
fn absolute_path(path: &Path) -> Result<()> {
    let text = path.to_str().ok_or("retained paths require UTF8")?;
    require(
        text.starts_with('/') && text != "/",
        "retained path must be absolute",
    )?;
    member_name(&text[1..])
}
fn validate_member_roster(members: &BTreeMap<String, Pin>) -> Result<()> {
    require(
        members.len() == MEMBERS.len(),
        "retained member roster differs",
    )?;
    for (name, size, sha) in MEMBERS {
        member_name(name)?;
        let pin = members.get(*name).ok_or("retained member missing")?;
        require(
            pin.size == *size && pin.sha256 == *sha,
            "retained member pin differs",
        )?;
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Node {
    dev: u64,
    ino: u64,
    mode: u32,
    uid: u32,
}
fn node(meta: &fs::Metadata) -> Node {
    Node {
        dev: meta.dev(),
        ino: meta.ino(),
        mode: meta.mode(),
        uid: meta.uid(),
    }
}
// Walk with directory FDs and NOFOLLOW at every component. Only one temporary
// directory FD is needed per walk; saved directory identities are rechecked.
fn walk(path: &Path, directory: bool) -> Result<(File, Vec<Node>)> {
    absolute_path(path)?;
    let parts = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut fd = File::open("/")?;
    let mut anchors = vec![node(&fd.metadata()?)];
    for (i, part) in parts.iter().enumerate() {
        let is_dir = i + 1 < parts.len() || directory;
        let flags = rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC
            | rustix::fs::OFlags::NONBLOCK
            | if is_dir {
                rustix::fs::OFlags::DIRECTORY
            } else {
                rustix::fs::OFlags::empty()
            };
        fd = File::from(rustix::fs::openat(
            &fd,
            Path::new(part),
            flags,
            rustix::fs::Mode::empty(),
        )?);
        let meta = fd.metadata()?;
        if is_dir {
            require(meta.is_dir(), "retained ancestor is not directory")?;
            anchors.push(node(&meta));
        }
    }
    Ok((fd, anchors))
}
struct HeldFile {
    path: PathBuf,
    file: File,
    anchors: Vec<Node>,
    limit: u64,
}
impl HeldFile {
    fn open(path: &Path, limit: u64) -> Result<Self> {
        let (file, anchors) = walk(path, false)?;
        let meta = file.metadata()?;
        require(
            meta.is_file()
                && meta.nlink() == 1
                && meta.mode() & 0o022 == 0
                && meta.uid() == fs::metadata("/proc/self")?.uid()
                && meta.len() <= limit,
            "retained input type/owner/mode/links/size invalid",
        )?;
        let held = Self {
            path: path.into(),
            file,
            anchors,
            limit,
        };
        held.snapshot()?;
        Ok(held)
    }
    fn bytes(&self) -> Result<Vec<u8>> {
        let size = self.file.metadata()?.len();
        require(size <= self.limit, "retained input grew past bound")?;
        let mut bytes = vec![0; usize::try_from(size)?];
        self.file.read_exact_at(&mut bytes, 0)?;
        let mut tail = [0];
        require(
            self.file.read_at(&mut tail, size)? == 0,
            "retained input grew while reading",
        )?;
        Ok(bytes)
    }
    fn snapshot(&self) -> Result<Value> {
        let (current, anchors) = walk(&self.path, false)?;
        let before = self.file.metadata()?;
        require(
            anchors == self.anchors
                && node(&before) == node(&current.metadata()?)
                && before.is_file()
                && before.nlink() == 1
                && before.len() <= self.limit
                && before.mode() & 0o022 == 0
                && before.uid() == fs::metadata("/proc/self")?.uid(),
            "retained path or ancestor changed",
        )?;
        let mut hash = Sha256::new();
        let mut offset = 0_u64;
        let mut buf = [0_u8; 65536];
        loop {
            let count = self.file.read_at(&mut buf, offset)?;
            if count == 0 {
                break;
            }
            offset += count as u64;
            require(offset <= self.limit, "retained input grew during hash")?;
            hash.update(&buf[..count]);
        }
        let after = self.file.metadata()?;
        let stamp = |m: &fs::Metadata| {
            (
                m.len(),
                m.mtime(),
                m.mtime_nsec(),
                m.ctime(),
                m.ctime_nsec(),
                m.nlink(),
                m.mode(),
            )
        };
        require(
            stamp(&before) == stamp(&after) && offset == before.len(),
            "retained input changed during hash",
        )?;
        Ok(
            json!({"path":self.path,"dev":before.dev(),"ino":before.ino(),"mode":before.mode(),"uid":before.uid(),"size":offset,"mtime":before.mtime(),"mtime_nsec":before.mtime_nsec(),"ctime":before.ctime(),"ctime_nsec":before.ctime_nsec(),"sha256":format!("{:x}",hash.finalize())}),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SourcePolicy {
    Present,
    Absent,
}
impl SourcePolicy {
    pub(super) fn parse(value: Option<&std::ffi::OsStr>) -> Result<Self> {
        match value.and_then(std::ffi::OsStr::to_str) {
            None if value.is_none() => Ok(Self::Present),
            Some("present") => Ok(Self::Present),
            Some("absent") => Ok(Self::Absent),
            _ => Err(crate::argparse_scan::ArgumentError(
                "--source-policy must be present or absent".into(),
            )
            .into()),
        }
    }
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Present => "present",
            Self::Absent => "absent",
        }
    }
}
const SOURCE_PARENT_ERROR: &str = "logging source parent boundary invalid";
fn source_node(meta: &fs::Metadata) -> (u64, u64, u32, u32, u32) {
    (meta.dev(), meta.ino(), meta.mode(), meta.uid(), meta.gid())
}
// Preserve the legacy ../.. spelling while refusing symlinks before canonicalize
// could hide them. No source file is opened by this directory-only walk.
fn source_root(path: &Path) -> Result<(PathBuf, File)> {
    let path = std::path::absolute(path).map_err(|_| SOURCE_PARENT_ERROR)?;
    let mut fd = File::open("/").map_err(|_| SOURCE_PARENT_ERROR)?;
    for part in path.components() {
        let name = match part {
            Component::RootDir | Component::CurDir => continue,
            Component::Normal(name) => name,
            Component::ParentDir => std::ffi::OsStr::new(".."),
            _ => return Err(SOURCE_PARENT_ERROR.into()),
        };
        fd = File::from(
            rustix::fs::openat(
                &fd,
                Path::new(name),
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::DIRECTORY
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::NONBLOCK
                    | rustix::fs::OFlags::CLOEXEC,
                rustix::fs::Mode::empty(),
            )
            .map_err(|_| SOURCE_PARENT_ERROR)?,
        );
    }
    let canonical = path.canonicalize().map_err(|_| SOURCE_PARENT_ERROR)?;
    let (current, _) = walk(&canonical, true).map_err(|_| SOURCE_PARENT_ERROR)?;
    require(
        source_node(&fd.metadata()?) == source_node(&current.metadata()?),
        SOURCE_PARENT_ERROR,
    )?;
    Ok((canonical, fd))
}
struct SourceParent {
    path: PathBuf,
    file: File,
    anchors: Vec<Node>,
    identity: (u64, u64, u32, u32, u32),
}
impl SourceParent {
    fn open(path: PathBuf) -> Result<Self> {
        let (file, anchors) = walk(&path, true).map_err(|_| SOURCE_PARENT_ERROR)?;
        let metadata = file.metadata()?;
        require(
            metadata.uid() == fs::metadata("/proc/self")?.uid() && metadata.mode() & 0o022 == 0,
            SOURCE_PARENT_ERROR,
        )?;
        Ok(Self {
            path,
            file,
            anchors,
            identity: source_node(&metadata),
        })
    }
    fn verify(&self) -> Result<()> {
        let (current, anchors) = walk(&self.path, true).map_err(|_| SOURCE_PARENT_ERROR)?;
        require(
            anchors == self.anchors
                && source_node(&current.metadata()?) == self.identity
                && source_node(&self.file.metadata()?) == self.identity,
            SOURCE_PARENT_ERROR,
        )
    }
}
pub(super) struct SourceGuard {
    requested: PathBuf,
    root: PathBuf,
    policy: SourcePolicy,
    parents: Vec<SourceParent>,
    present: Vec<(HeldFile, Value)>,
    failure: std::cell::RefCell<Option<String>>,
}
impl SourceGuard {
    pub(super) fn open(root: &Path, policy: SourcePolicy) -> Result<Self> {
        let requested = std::path::absolute(root).map_err(|_| SOURCE_PARENT_ERROR)?;
        let (root, raw) = source_root(&requested)?;
        let parents = [root.clone(), root.join("rust"), root.join("rust/tools")]
            .into_iter()
            .map(SourceParent::open)
            .collect::<Result<Vec<_>>>()?;
        require(
            source_node(&raw.metadata()?) == parents[0].identity,
            SOURCE_PARENT_ERROR,
        )?;
        let mut present = Vec::new();
        if policy == SourcePolicy::Present {
            for name in SOURCE_LEAVES {
                let file = HeldFile::open(&root.join(name), 2 * 1024 * 1024)
                    .map_err(|_| format!("logging source present file invalid: {name}"))?;
                let before = file
                    .snapshot()
                    .map_err(|_| format!("logging source present file invalid: {name}"))?;
                present.push((file, before));
            }
        }
        let guard = Self {
            requested,
            root,
            policy,
            parents,
            present,
            failure: std::cell::RefCell::new(None),
        };
        guard.verify()?;
        Ok(guard)
    }
    pub(super) fn root(&self) -> &Path {
        &self.root
    }
    pub(super) fn verify(&self) -> Result<()> {
        if let Some(error) = self.failure.borrow().as_ref() {
            return Err(error.clone().into());
        }
        let result = self.verify_current();
        if let Err(error) = &result {
            *self.failure.borrow_mut() = Some(error.to_string());
        }
        result
    }
    fn verify_current(&self) -> Result<()> {
        let (root, current) = source_root(&self.requested)?;
        require(
            root == self.root && source_node(&current.metadata()?) == self.parents[0].identity,
            SOURCE_PARENT_ERROR,
        )?;
        for parent in &self.parents {
            parent.verify()?;
        }
        match self.policy {
            SourcePolicy::Present => {
                for ((file, before), name) in self.present.iter().zip(SOURCE_LEAVES) {
                    let after = file
                        .snapshot()
                        .map_err(|_| format!("logging source present file changed: {name}"))?;
                    require(
                        &after == before,
                        &format!("logging source present file changed: {name}"),
                    )?;
                }
            }
            SourcePolicy::Absent => {
                let parent = &self.parents[2];
                for name in SOURCE_LEAVES {
                    let leaf = Path::new(name).file_name().ok_or(SOURCE_PARENT_ERROR)?;
                    match rustix::fs::statat(
                        &parent.file,
                        Path::new(leaf),
                        rustix::fs::AtFlags::SYMLINK_NOFOLLOW,
                    ) {
                        Err(rustix::io::Errno::NOENT) => {}
                        Ok(_) => {
                            return Err(format!("logging retired source reappeared: {name}").into())
                        }
                        Err(_) => {
                            return Err(
                                format!("logging source absence inspection failed: {name}").into()
                            )
                        }
                    }
                    match fs::symlink_metadata(self.root.join(name)) {
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Ok(_) => {
                            return Err(format!("logging retired source reappeared: {name}").into())
                        }
                        Err(_) => {
                            return Err(
                                format!("logging source absence inspection failed: {name}").into()
                            )
                        }
                    }
                }
            }
        }
        for parent in &self.parents {
            parent.verify()?;
        }
        Ok(())
    }
    pub(super) fn complete<T>(&self, actual: Result<T>) -> Result<T> {
        checked_result(actual, self.verify())
    }
}
// Always evaluate the final check, but never replace the first command failure.
pub(super) fn checked_result<T>(actual: Result<T>, final_check: Result<()>) -> Result<T> {
    match actual {
        Err(error) => {
            if let Err(secondary) = final_check {
                eprintln!("logging final check after failure: {secondary}");
            }
            Err(error)
        }
        Ok(value) => {
            final_check?;
            Ok(value)
        }
    }
}

fn layout(root: &Path, names: &BTreeSet<String>) -> Result<()> {
    let mut pending = vec![root.to_path_buf()];
    let mut actual = BTreeSet::new();
    let mut directories = 0;
    while let Some(directory) = pending.pop() {
        directories += 1;
        require(
            directories <= 128,
            "retained context directory bound exceeded",
        )?;
        let (held_directory, _) = walk(&directory, true)?;
        for row in fs::read_dir(format!("/proc/self/fd/{}", held_directory.as_raw_fd()))? {
            let row = row?;
            let path = directory.join(row.file_name());
            let relative = path
                .strip_prefix(root)?
                .to_str()
                .ok_or("retained member UTF8")?
                .to_owned();
            member_name(&relative)?;
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.is_dir() {
                require(
                    names
                        .iter()
                        .any(|name| name.starts_with(&format!("{relative}/"))),
                    "unlisted retained directory",
                )?;
                pending.push(path);
            } else {
                require(
                    metadata.is_file() && names.contains(&relative),
                    "unlisted or nonregular retained member",
                )?;
                actual.insert(relative);
                require(
                    actual.len() <= names.len(),
                    "retained member count exceeded",
                )?;
            }
        }
    }
    require(&actual == names, "retained on-disk roster differs")
}
struct Inputs {
    held: BTreeMap<String, HeldFile>,
    pins: BTreeMap<String, Pin>,
    before: Value,
    index: HeldFile,
    index_before: Value,
    names: BTreeSet<String>,
    directory: PathBuf,
}
impl Inputs {
    fn open(path: &Path, root: &Path) -> Result<Self> {
        let parent = path.parent().ok_or("retained index parent missing")?;
        let (directory, _) = walk(parent, true)?;
        let meta = directory.metadata()?;
        require(
            meta.uid() == fs::metadata("/proc/self")?.uid() && meta.mode() & 0o077 == 0,
            "retained context root must be owned private0700",
        )?;
        let index = HeldFile::open(path, 128 * 1024)?;
        let index_before = index.snapshot()?;
        let parsed = parse_index(&index.bytes()?)?;
        require(
            parsed.schema == 1
                && parsed.recovery_run == 39894886676
                && parsed.recovery_artifact == 12477566277
                && parsed.recovery_zip_sha256 == RECOVERY_ZIP
                && parsed.proof_gzip_sha256 == PROOF_GZIP,
            "retained context identity/schema differs",
        )?;
        validate_member_roster(&parsed.members)?;
        require(
            parsed.source_files.len() == SOURCE_FILES.len()
                && SOURCE_FILES
                    .iter()
                    .all(|s| parsed.source_files.contains_key(*s)),
            "fresh source import roster differs",
        )?;
        let mut names = parsed.members.keys().cloned().collect::<BTreeSet<_>>();
        let index_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("retained index name missing")?;
        require(
            names.insert(index_name.to_owned()),
            "retained index overlaps a member",
        )?;
        layout(parent, &names)?;
        let mut held = BTreeMap::new();
        let mut pins = BTreeMap::new();
        for (name, pin) in parsed.members {
            let file = HeldFile::open(&parent.join(&name), pin.size)?;
            let snapshot = file.snapshot()?;
            require(
                snapshot["size"] == pin.size && snapshot["sha256"] == pin.sha256,
                "retained member content differs",
            )?;
            pins.insert(name.clone(), pin);
            held.insert(name, file);
        }
        for (name, pin) in parsed.source_files {
            member_name(&name)?;
            require(
                pin.size <= 2 * 1024 * 1024 && hex(&pin.sha256, 64),
                "fresh source pin invalid",
            )?;
            let file = HeldFile::open(&root.join(&name), pin.size)?;
            let snapshot = file.snapshot()?;
            require(
                snapshot["size"] == pin.size && snapshot["sha256"] == pin.sha256,
                "fresh source content differs",
            )?;
            pins.insert(format!("current/{name}"), pin);
            held.insert(format!("current/{name}"), file);
        }
        let mut inputs = Self {
            held,
            pins,
            before: Value::Null,
            index,
            index_before,
            names,
            directory: parent.into(),
        };
        inputs.before = inputs.snapshot()?;
        require(
            inputs.index.snapshot()? == inputs.index_before,
            "retained index changed during admission",
        )?;
        Ok(inputs)
    }
    fn bytes(&self, name: &str) -> Result<Vec<u8>> {
        self.held.get(name).ok_or("retained role missing")?.bytes()
    }
    fn json(&self, name: &str) -> Result<Value> {
        Ok(serde_json::from_slice(&self.bytes(name)?)?)
    }
    fn text(&self, name: &str) -> Result<String> {
        Ok(String::from_utf8(self.bytes(name)?)?)
    }
    fn snapshot(&self) -> Result<Value> {
        layout(&self.directory, &self.names)?;
        let mut result = BTreeMap::new();
        result.insert("index".to_owned(), self.index.snapshot()?);
        for (name, file) in &self.held {
            let snapshot = file.snapshot()?;
            let pin = self.pins.get(name).ok_or("retained pin missing")?;
            require(
                snapshot["size"] == pin.size && snapshot["sha256"] == pin.sha256,
                "retained content no longer matches pinned input",
            )?;
            result.insert(name.clone(), snapshot);
        }
        Ok(serde_json::to_value(result)?)
    }
    fn unchanged(&self) -> Result<Value> {
        let after = self.snapshot()?;
        require(
            after == self.before && self.index.snapshot()? == self.index_before,
            "retained source or input changed",
        )?;
        Ok(after)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedPacket {
    raw_base64: String,
    record: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Invocation {
    binary: String,
    binary_sha256: String,
    cwd: String,
    endpoint: String,
    original: bool,
    source_commit: String,
    source_tree: String,
    started: f64,
    #[serde(default)]
    pid: Option<u32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    command: Value,
    response: Value,
}
struct Imported {
    packets: Vec<Vec<u8>>,
    responses: Vec<Response>,
    stdout: Vec<u8>,
    filename: String,
}

#[allow(clippy::too_many_arguments)] // Explicit independent lifetime/callsite/provenance inputs.
fn wire_record(
    raw: &[u8],
    saved: &Value,
    started: f64,
    finished: f64,
    filename: &str,
    source: &str,
    original: bool,
    commit: &str,
    tree: &str,
) -> Result<()> {
    require(
        !raw.is_empty() && raw.len() <= 1 << 20,
        "retained wire size invalid",
    )?;
    let record: Value = serde_json::from_slice(&raw[1..])?;
    require(&record == saved, "retained raw/parsed record differs")?;
    let object = record.as_object().ok_or("retained wire is not object")?;
    let keys = [
        "created", "ctx", "filename", "funcname", "levelnum", "lineno", "msg",
    ];
    require(
        object.len() == 7
            && keys.iter().all(|k| object.contains_key(*k))
            && record["levelnum"].as_u64() == Some(raw[0] as u64),
        "retained wire schema/level differs",
    )?;
    let created = record["created"]
        .as_f64()
        .ok_or("retained timestamp missing")?;
    require(
        started.is_finite() && finished.is_finite() && started <= created && created <= finished,
        "retained timestamp outside recorded bounds",
    )?;
    require(
        record["filename"] == filename && record["msg"].is_string() && record["ctx"].is_object(),
        "retained source/context differs",
    )?;
    let line = usize::try_from(record["lineno"].as_u64().ok_or("retained line missing")?)?;
    let site = line
        .checked_sub(1)
        .and_then(|i| source.lines().nth(i))
        .ok_or("retained source line outside file")?;
    require(
        site.contains(if original { "cloudlog" } else { "logger.emit" }),
        "retained source line is not callsite",
    )?;
    require(
        if original {
            record["funcname"] == "emit" || record["funcname"] == "rate"
        } else {
            record["funcname"] == "native_logging_probe::emit"
                && record["ctx"]["runtime_language"] == "rust"
                && record["ctx"]["source_commit"] == commit
                && record["ctx"]["source_tree"] == tree
        },
        "retained function/provenance differs",
    )
}
fn imported(inputs: &Inputs, prefix: &str, historical: bool, end: f64) -> Result<Imported> {
    let invocation: Invocation =
        serde_json::from_slice(&inputs.bytes(&format!("{prefix}/invocation.json"))?)?;
    let (root, binary, sha, commit) = if historical {
        (OLD_ROOT, OLD_BINARY, OLD_ELF, OLD_SOURCE)
    } else {
        (CAPTURE_ROOT, CAPTURE_BINARY, CAPTURE_ELF, CAPTURE_SOURCE)
    };
    require(
        invocation.original
            && invocation.binary == binary
            && invocation.binary_sha256 == sha
            && invocation.cwd == root
            && invocation.source_commit == commit
            && invocation.source_tree == "clean"
            && invocation.endpoint.starts_with("ipc:///tmp/")
            && invocation.started.is_finite()
            && invocation.started <= end
            && (historical || invocation.pid.is_some()),
        "retained invocation identity differs",
    )?;
    let filename = format!("{root}/{CPP_SITE}");
    let source = inputs.text("reference/native_logging_reference.cc")?;
    let rows: Vec<SavedPacket> =
        serde_json::from_slice(&inputs.bytes(&format!("{prefix}/packets.json"))?)?;
    let mut packets = Vec::new();
    for row in rows {
        let raw = STANDARD.decode(&row.raw_base64)?;
        wire_record(
            &raw,
            &row.record,
            invocation.started,
            end,
            &filename,
            &source,
            true,
            "",
            "",
        )?;
        packets.push(raw);
    }
    let responses: Vec<Response> =
        serde_json::from_slice(&inputs.bytes(&format!("{prefix}/responses.json"))?)?;
    let stream = inputs.text(&format!("{prefix}/responses.jsonl"))?;
    let lines = stream
        .lines()
        .map(serde_json::from_str::<Value>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    require(
        Value::Array(lines) == inputs.json(&format!("{prefix}/responses.json"))?,
        "retained response JSON/JSONL differs",
    )?;
    let stdout = if prefix.ends_with("/console-failure") {
        Vec::new()
    } else {
        inputs.bytes(&format!("{prefix}/stdout.txt"))?
    };
    Ok(Imported {
        packets,
        responses,
        stdout,
        filename,
    })
}
fn equal_packets(left: &[Vec<u8>], right: &[Vec<u8>], threads: bool) -> Result<()> {
    require(
        !left.is_empty() && left.len() == right.len(),
        "retained packet count differs",
    )?;
    if threads {
        let a = left.iter().collect::<BTreeSet<_>>();
        let b = right.iter().collect::<BTreeSet<_>>();
        require(
            left.len() == 64 && a.len() == 64 && b.len() == 64 && a == b,
            "retained unique thread bytes differ",
        )
    } else {
        require(left == right, "retained ordered wire bytes differ")
    }
}
fn project(packets: &[Vec<u8>]) -> Result<Vec<Vec<u8>>> {
    packets.iter().map(|p| normalize(p)).collect()
}
fn bridge(left: &[u8], left_name: &str, right: &[u8], right_name: &str) -> Result<Value> {
    require(
        !left_name.is_empty() && !right_name.is_empty(),
        "retained console callsite missing",
    )?;
    let a = std::str::from_utf8(left)?;
    let b = std::str::from_utf8(right)?;
    let from_a = format!("{left_name}: ");
    let from_b = format!("{right_name}: ");
    require(
        a.replace(&from_a, "FILE: ") == b.replace(&from_b, "FILE: "),
        "retained console projected bytes differ",
    )?;
    Ok(
        json!({"left_filename":left_name,"right_filename":right_name,"left_sha256":format!("{:x}",Sha256::digest(left)),"right_sha256":format!("{:x}",Sha256::digest(right)),"left_bytes":left.len(),"right_bytes":right.len(),"raw_equal":left == right,"projected_equal":true,"left_replacements":a.matches(&from_a).count(),"right_replacements":b.matches(&from_b).count()}),
    )
}

pub(super) fn selected(args: &[OsString]) -> bool {
    args.first().is_some_and(|arg| arg == "retained")
}
fn absent_output(output: &Path) -> Result<()> {
    absolute_path(output)?;
    match fs::symlink_metadata(output) {
        Ok(_) => return Err("logging evidence output already exists".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    let (parent, _) = walk(
        output.parent().ok_or("retained output parent missing")?,
        true,
    )?;
    let meta = parent.metadata()?;
    require(
        meta.uid() == fs::metadata("/proc/self")?.uid() && meta.mode() & 0o077 == 0,
        "retained output parent must be owned private0700",
    )
}
fn save(output: &File, name: &str, value: &Value) -> Result<()> {
    member_name(name)?;
    require(!name.contains('/'), "retained receipt name is not a leaf")?;
    let mut file = File::from(rustix::fs::openat(
        output,
        name,
        rustix::fs::OFlags::WRONLY
            | rustix::fs::OFlags::CREATE
            | rustix::fs::OFlags::EXCL
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )?);
    file.write_all(&serde_json::to_vec_pretty(value)?)?;
    file.write_all(b"\n")?;
    Ok(())
}
pub(super) fn cli(args: &[OsString]) -> Result<()> {
    let result = run_args(args);
    // Same cancellation/reap normalization as the legacy path; no new runner.
    lifecycle::reaped()?;
    lifecycle::check()?;
    result
}
fn run_args(args: &[OsString]) -> Result<()> {
    let parsed = scan_os_required_first(
        args,
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--context", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--root", true),
            OptionSpec::value("--source-commit", true),
            OptionSpec::value("--binary-sha256", true),
            OptionSpec::value("--source-policy", false),
        ],
    )
    .map_err(|error| {
        if error.0 == "unrecognized arguments" {
            for forbidden in ["--builder", "--reference", "--source-hook"] {
                let inline = format!("{forbidden}=");
                if args.iter().any(|arg| {
                    arg == forbidden || arg.as_encoded_bytes().starts_with(inline.as_bytes())
                }) {
                    return crate::argparse_scan::ArgumentError(format!(
                        "unrecognized arguments: {forbidden}"
                    ));
                }
            }
        }
        error
    })?;
    if parsed.help {
        println!(
            "usage: check-native-logging retained --binary BINARY --root ROOT --context INDEX --output OUTPUT --source-commit SHA40 --binary-sha256 SHA256 [--source-policy present|absent]"
        );
        return Ok(());
    }
    let output = PathBuf::from(&parsed.values["--output"]);
    absent_output(&output)?;
    let root = PathBuf::from(&parsed.values["--root"]);
    absolute_path(&root)?;
    let binary = PathBuf::from(&parsed.values["--binary"]);
    absolute_path(&binary)?;
    let context = PathBuf::from(&parsed.values["--context"]);
    absolute_path(&context)?;
    let commit = parsed.values["--source-commit"]
        .to_str()
        .ok_or("retained commit is not UTF8")?;
    let binary_sha = parsed.values["--binary-sha256"]
        .to_str()
        .ok_or("retained ELF hash is not UTF8")?;
    require(
        hex(commit, 40) && hex(binary_sha, 64),
        "retained source/provider pin invalid",
    )?;
    fresh_provider_pin(binary_sha)?;
    let policy = SourcePolicy::parse(
        parsed
            .values
            .get("--source-policy")
            .map(OsString::as_os_str),
    )?;
    let source_guard = SourceGuard::open(&root, policy)?;
    let actual = (|| -> Result<()> {
        let inputs = Inputs::open(&context, &root)?;
        let provider_file = HeldFile::open(&binary, 512 * 1024 * 1024)?;
        let provider = HeldBinary::open(&binary)?;
        let provider_before = provider.snapshot()?;
        let provider_file_before = provider_file.snapshot()?;
        require(
            provider_before["sha256"] == binary_sha
                && provider_file_before["sha256"] == binary_sha
                && provider_before["dev"] == provider_file_before["dev"]
                && provider_before["ino"] == provider_file_before["ino"],
            "fresh native ELF pin differs",
        )?;
        let corpus = Corpus::load(&inputs)?; // Every inert predicate precedes native traffic.
        inputs.unchanged()?;
        require(
            provider.snapshot()? == provider_before,
            "native provider changed before launch",
        )?;
        let _cancel = lifecycle::Cancel::new()?;
        source_guard.verify()?;
        let source_before = source_state(&root, commit)?;
        use std::os::unix::fs::DirBuilderExt;
        source_guard.verify()?;
        fs::DirBuilder::new().mode(0o700).create(&output)?;
        let (output_directory, output_parents) = walk(&output, true)?;
        save(
            &output_directory,
            "providers.before.json",
            &json!([provider_before]),
        )?;
        save(
            &output_directory,
            "context-inputs.before.json",
            &inputs.before,
        )?;
        save(
            &output_directory,
            "source-state.before.json",
            &source_before,
        )?;
        // Execute only the held, freshly built native inode. This descriptor is
        // inherited until exec; CLOEXEC closes it after the kernel loads the ELF.
        let launch = PathBuf::from(format!("/proc/self/fd/{}", provider.file.as_raw_fd()));
        source_guard.verify()?;
        let actual = compare_native(&corpus, &inputs, &launch, &root, &output, commit);
        let final_checks = (|| -> Result<()> {
            lifecycle::reaped()?;
            lifecycle::check()?;
            let source_after = source_state(&root, commit)?;
            require(
                source_before == source_after,
                "fresh source root/state changed",
            )?;
            let provider_after = provider.snapshot()?;
            let inputs_after = inputs.unchanged()?;
            let (output_after, parents_after) = walk(&output, true)?;
            require(
                output_parents == parents_after
                    && node(&output_directory.metadata()?) == node(&output_after.metadata()?),
                "retained output root changed",
            )?;
            require(
                provider_after == provider_before
                    && provider_file.snapshot()? == provider_file_before,
                "fresh native provider changed",
            )?;
            save(
                &output_directory,
                "providers.after.json",
                &json!([provider_after]),
            )?;
            save(
                &output_directory,
                "context-inputs.after.json",
                &inputs_after,
            )?;
            save(&output_directory, "source-state.after.json", &source_after)?;
            Ok(())
        })();
        let mut report = checked_result(actual, final_checks)?;
        report["result"] = json!("pass: retained native comparison only");
        report["R4_acceptance"] = json!(false);
        report["lifecycle_reuse_granted"] = json!(false);
        report["source_commit"] = json!(commit);
        report["native_binary_sha256"] = json!(binary_sha);
        report["native_exec_path"] = json!(launch);
        report["recovery_run"] = json!(39894886676_u64);
        report["cpp_executions"] = json!(0);
        report["limits"] = json!([
            "same-UID visible checks; no hostile same-UID/privileged mutation exclusion",
            "historical original root inode unrecorded; recovery did not revalidate current source",
            "import source roster is not complete Cargo/DSO/build provenance",
            "old units/grammar/31 controls/17 cases remain conditionally attributed to their original contexts; none reused by this command",
            "lifecycle case ledger, exact descendant trace, complete seal/upload and parent admission remain external R4 gates",
            "UTF8 host and existing fixture/projection scope only"
        ]);
        source_guard.verify()?;
        report["source_policy"] = json!(policy.as_str());
        save(&output_directory, "report.json", &report)?;
        println!("{report}");
        Ok(())
    })();
    match actual {
        Ok(()) => Ok(()),
        Err(error) => source_guard.complete(Err(error)),
    }
}
fn source_state(root: &Path, commit: &str) -> Result<Value> {
    require(
        String::from_utf8(git(root, &["rev-parse", "HEAD"])?)?.trim() == commit
            && git(root, &["status", "--porcelain", "--untracked-files=normal"])?.is_empty(),
        "retained fresh source HEAD/status differs",
    )?;
    let (directory, _) = walk(root, true)?;
    let metadata = directory.metadata()?;
    require(
        metadata.uid() == fs::metadata("/proc/self")?.uid() && metadata.mode() & 0o022 == 0,
        "fresh source root owner/mode invalid",
    )?;
    Ok(
        json!({"head":commit,"status":"","root":root,"dev":metadata.dev(),"ino":metadata.ino(),"mode":metadata.mode(),"uid":metadata.uid(),"held_source_files":SOURCE_FILES.len(),"complete_build_dependency_closure":false}),
    )
}

fn console_commands() -> Vec<Value> {
    let mut commands = Vec::new();
    for level in [0, 10, 20, 30, 40, 50] {
        for text in [
            "plain",
            "한글😀é\u{2028}\u{2029}\"\\\u{8}\u{c}\n\r\t\u{1}",
            "\0tail",
            "prefix\0tail",
        ] {
            commands.push(json!({"op":"emit","level":level,"text":text}));
        }
    }
    commands.push(json!({"op":"emit","level":40,"text":""}));
    commands
}
fn cpp_response(response: &Value, sent: u64) -> Result<()> {
    require(
        *response == json!({"sent":sent,"dropped":0,"linger":100,"errors":0}),
        "retained original response differs",
    )
}
fn console_inputs(peer: &Imported) -> Result<()> {
    let commands = console_commands();
    require(
        peer.packets.len() == 24 && peer.responses.len() == 25,
        "retained console fixture count differs",
    )?;
    for (i, (row, command)) in peer.responses.iter().zip(commands).enumerate() {
        require(row.command == command, "retained console input differs")?;
        cpp_response(&row.response, u64::from(i != 24))?;
        if i < 24 {
            let record: Value = serde_json::from_slice(&peer.packets[i][1..])?;
            let text = command["text"]
                .as_str()
                .ok_or("retained console text missing")?;
            require(
                record["levelnum"] == command["level"]
                    && record["msg"] == text.split('\0').next().unwrap_or(""),
                "retained console payload differs",
            )?;
        }
    }
    Ok(())
}
struct Corpus {
    console: Vec<Imported>,
    old_console: Vec<Imported>,
    rate: Imported,
    threads: Imported,
    failure: Imported,
    bridges: Vec<Value>,
    end: f64,
}
impl Corpus {
    fn load(inputs: &Inputs) -> Result<Self> {
        let capture = inputs.json("recovered/capture-complete.json")?;
        require(
            capture["source"] == CAPTURE_SOURCE
                && capture["run"] == 39888221765_u64
                && capture["root"] == CAPTURE_ROOT
                && capture["C++_packets"] == 249
                && capture["C++_responses"] == 694
                && capture["all_packets"] == 499
                && capture["all_responses"] == 1391
                && capture["exec"] == 86
                && capture["cwd"] == 112
                && capture["R4_acceptance"] == false
                && capture["lifecycle_reuse_granted"] == false
                && capture["historical_provider_relabelled"] == false
                && capture["console_cross_root_equivalence_admitted"] == false,
            "retained capture role/counts differ",
        )?;
        let before = inputs.json("recovered/providers.before.json")?;
        require(
            before == inputs.json("recovered/providers.after.json")?
                && before.as_array().is_some_and(|a| a.len() == 2)
                && before[0]["path"] == CAPTURE_BINARY
                && before[0]["sha256"] == CAPTURE_ELF
                && before[0]["size"] == 135632,
            "retained provider receipt differs",
        )?;
        let old_before = inputs.json("historical/providers.before.json")?;
        require(
            old_before == inputs.json("historical/providers.after.json")?
                && old_before[0]["path"] == OLD_BINARY
                && old_before[0]["sha256"] == OLD_ELF
                && old_before[0]["size"] == 135632,
            "historical provider receipt differs",
        )?;
        let state = inputs.json("recovered/source-state.before.json")?;
        require(
            state == inputs.json("recovered/source-state.after.json")?
                && state["head"] == CAPTURE_SOURCE
                && state["status"] == "",
            "retained source receipt differs",
        )?;
        let identity = inputs.json("recovered/reference-identity.json")?;
        require(
            identity["actual_sha256"] == CAPTURE_ELF
                && identity["immutable_old_sha256"] == OLD_ELF
                && identity["actual_root"] == CAPTURE_ROOT
                && identity["old_root"] == OLD_ROOT
                && identity["equals_old"] == false
                && identity["same_source_provenance"] == true,
            "retained original context identity differs",
        )?;
        let provenance = inputs.json("recovered/reference-provenance.json")?;
        require(
            provenance["sources"][CPP_SITE]
                == format!(
                    "{:x}",
                    Sha256::digest(inputs.bytes("reference/native_logging_reference.cc")?)
                ),
            "retained source-callsite provenance differs",
        )?;
        let result = inputs.json("recovered/protocol-result.json")?;
        let end = result["finished_walltime"]
            .as_f64()
            .ok_or("retained finish missing")?;
        require(
            result["returncode"] == 0
                && result["cwd"] == format!("{CAPTURE_ROOT}/rust")
                && end.is_finite()
                && result["started_walltime"]
                    .as_f64()
                    .is_some_and(|s| s <= end),
            "retained protocol terminal differs",
        )?;
        let api = inputs.json("historical/api.json")?;
        require(
            api["id"] == 12442250376_u64
                && api["workflow_run"]["id"] == 39813703320_u64
                && api["workflow_run"]["repository_id"] == 1395501963_u64
                && api["workflow_run"]["head_repository_id"] == 1395501963_u64
                && api["digest"]
                    == "sha256:6eddbcdae19c54cd6c19ad740ae1b881b59cb16261e9f9d7bd3878382dc1b6a5"
                && api["created_at"] == "2026-10-22T16:29:44Z",
            "historical API identity differs",
        )?;
        // R3 has no per-peer end timestamp. Its already admitted API publication
        // time is an upper bound, not an invented producer lifetime receipt.
        let old_end = 1_792_686_584.0;
        let mut console = Vec::new();
        let mut old_console = Vec::new();
        let mut bridges = Vec::new();
        for i in 0..7 {
            let new = imported(inputs, &format!("recovered/console-{i}"), false, end)?;
            let old = imported(inputs, &format!("historical/console-{i}"), true, old_end)?;
            console_inputs(&new)?;
            console_inputs(&old)?;
            equal_packets(&project(&old.packets)?, &project(&new.packets)?, false)?;
            bridges.push(bridge(
                &old.stdout,
                &old.filename,
                &new.stdout,
                &new.filename,
            )?);
            console.push(new);
            old_console.push(old);
        }
        let rate = imported(inputs, "recovered/rate", false, end)?;
        require(
            rate.responses.len() == 516 && rate.packets.len() == 16,
            "retained rate counts differ",
        )?;
        let mut delivered = 0;
        for (row, timestamp) in rate.responses.iter().zip(rate_timestamps()) {
            require(
                row.command
                    == json!({"op":"rate","timestamp":timestamp.to_string(),"text":format!("rate-{timestamp}")}),
                "retained rate input differs",
            )?;
            let sent = row.response["sent"]
                .as_u64()
                .ok_or("retained rate sent missing")?;
            require(sent <= 2, "retained rate count exceeds bound")?;
            cpp_response(&row.response, sent)?;
            delivered += sent;
        }
        require(
            delivered == 16,
            "retained rate response/packet count differs",
        )?;
        let threads = imported(inputs, "recovered/transport/threads", false, end)?;
        require(
            threads.packets.len() == 64
                && threads.responses.len() == 1
                && threads.responses[0].command == json!({"op":"threads","count":64}),
            "retained thread fixture differs",
        )?;
        cpp_response(&threads.responses[0].response, 64)?;
        let messages = threads
            .packets
            .iter()
            .map(|raw| {
                Ok(serde_json::from_slice::<Value>(&raw[1..])?["msg"]
                    .as_str()
                    .ok_or("retained thread message missing")?
                    .to_owned())
            })
            .collect::<Result<BTreeSet<_>>>()?;
        require(
            messages == (0..64).map(|i| format!("thread-{i}")).collect(),
            "retained thread uniqueness differs",
        )?;
        let failure = imported(inputs, "recovered/transport/console-failure", false, end)?;
        require(
            failure.packets.len() == 1
                && failure.responses.len() == 1
                && failure.responses[0].command
                    == json!({"op":"emit","level":40,"text":"console failure remains nonfatal"}),
            "retained console-failure fixture differs",
        )?;
        cpp_response(&failure.responses[0].response, 1)?;
        let pressure = imported(inputs, "recovered/transport/backpressure", false, end)?;
        require(
            pressure.packets.is_empty()
                && pressure.responses.len() == 1
                && pressure.responses[0].command == json!({"op":"flood","count":5000}),
            "retained backpressure fixture differs",
        )?;
        let response = &pressure.responses[0].response;
        require(
            response["sent"].as_u64().is_some_and(|n| n > 0)
                && response["dropped"].as_u64().is_some_and(|n| n > 0)
                && response["sent"]
                    .as_u64()
                    .zip(response["dropped"].as_u64())
                    .is_some_and(|(s, d)| s + d == 5000)
                && response["errors"] == 0
                && response["linger"] == 100,
            "retained backpressure accounting differs",
        )?;
        let timing = inputs.json("recovered/transport/backpressure/timing.json")?;
        require(
            timing["backpressure"] == *response
                && timing["send_and_shutdown_seconds"]
                    .as_f64()
                    .is_some_and(|s| (0.0..2.0).contains(&s)),
            "retained backpressure bound differs",
        )?;
        for (label, code) in [("SIGINT", -2), ("SIGTERM", -15)] {
            let prefix = format!("recovered/transport/{label}");
            let invocation: Invocation =
                serde_json::from_slice(&inputs.bytes(&format!("{prefix}/invocation.json"))?)?;
            require(
                invocation.original
                    && invocation.binary == CAPTURE_BINARY
                    && invocation.binary_sha256 == CAPTURE_ELF
                    && invocation.cwd == CAPTURE_ROOT
                    && invocation.source_commit == CAPTURE_SOURCE
                    && invocation.source_tree == "clean",
                "retained signal provider differs",
            )?;
            let signal = inputs.json(&format!("{prefix}/signal.json"))?;
            require(
                signal["signal"] == label
                    && signal["returncode"] == code
                    && signal["seconds"]
                        .as_f64()
                        .is_some_and(|s| (0.0..2.0).contains(&s)),
                "retained signal bound differs",
            )?;
        }
        let empty = inputs.json("recovered/transport/empty-invalid-endpoint.json")?;
        let nonempty = inputs.json("recovered/transport/nonempty-invalid-endpoint.json")?;
        require(
            empty["returncode"] == 0
                && nonempty["returncode"] == 0
                && serde_json::from_str::<Value>(
                    empty["stderr"]
                        .as_str()
                        .ok_or("retained invalid stderr missing")?,
                )?["sent"]
                    == 0
                && serde_json::from_str::<Value>(
                    nonempty["stderr"]
                        .as_str()
                        .ok_or("retained invalid stderr missing")?,
                )?["dropped"]
                    == 1,
            "retained original invalid-endpoint asymmetry differs",
        )?;
        let packets = console.iter().map(|p| p.packets.len()).sum::<usize>()
            + rate.packets.len()
            + threads.packets.len()
            + failure.packets.len();
        let responses = console.iter().map(|p| p.responses.len()).sum::<usize>()
            + rate.responses.len()
            + threads.responses.len()
            + failure.responses.len()
            + pressure.responses.len();
        require(
            (packets, responses) == (249, 694),
            "retained original corpus incomplete",
        )?;
        Ok(Self {
            console,
            old_console,
            rate,
            threads,
            failure,
            bridges,
            end,
        })
    }
}
fn fresh_peer(peer: &Peer, inputs: &Inputs, root: &Path, commit: &str) -> Result<()> {
    require(
        peer.commit == commit && peer.tree == "clean" && !peer.original && peer.root == root,
        "fresh peer source differs",
    )?;
    let text = inputs.text(&format!("current/{PROBE}"))?;
    let relative = PROBE
        .strip_prefix("rust/")
        .ok_or("fresh probe path invalid")?;
    for row in &peer.packets {
        let raw = STANDARD.decode(
            row["raw_base64"]
                .as_str()
                .ok_or("fresh packet bytes missing")?,
        )?;
        wire_record(
            &raw,
            &row["record"],
            peer.started,
            realtime()?,
            relative,
            &text,
            false,
            commit,
            "clean",
        )?;
    }
    Ok(())
}
fn compare_responses(original: &Imported, fresh: &[Value], rate: bool) -> Result<()> {
    require(
        original.responses.len() == fresh.len(),
        "retained fresh response count differs",
    )?;
    for (a, b) in original.responses.iter().zip(fresh) {
        let mut command = a.command.clone();
        if rate {
            command["timestamp"] = json!(command["timestamp"]
                .as_str()
                .ok_or("retained rate timestamp missing")?
                .parse::<u64>()?);
        }
        let filtered = u64::from(command["op"] == "emit" && command["text"] == "");
        require(
            command == b["command"]
                && b["response"]
                    == json!({"sent":a.response["sent"],"dropped":a.response["dropped"],"filtered":filtered}),
            "retained fresh command/response differs",
        )?;
    }
    Ok(())
}
fn fresh_transport(
    inputs: &Inputs,
    directory: &Path,
    case: &str,
    root: &Path,
    commit: &str,
    binary: &Path,
    binary_sha256: &str,
) -> Result<Vec<Vec<u8>>> {
    let prefix = directory.join(case);
    let invocation: Invocation =
        serde_json::from_slice(&fs::read(prefix.join("invocation.json"))?)?;
    require(
        !invocation.original
            && invocation.binary == binary.to_str().ok_or("fresh provider UTF8")?
            && invocation.binary_sha256 == binary_sha256
            && invocation.cwd == root.to_str().ok_or("fresh root UTF8")?
            && invocation.source_commit == commit
            && invocation.source_tree == "clean",
        "fresh transport invocation source differs",
    )?;
    let rows: Vec<SavedPacket> = serde_json::from_slice(&fs::read(prefix.join("packets.json"))?)?;
    let source = inputs.text(&format!("current/{PROBE}"))?;
    let filename = PROBE
        .strip_prefix("rust/")
        .ok_or("fresh probe path invalid")?;
    let mut packets = Vec::new();
    for (i, row) in rows.into_iter().enumerate() {
        let raw = STANDARD.decode(row.raw_base64)?;
        require(
            raw == fs::read(prefix.join(format!("packet-{i:04}.bin")))?,
            "fresh transport raw/base64 differs",
        )?;
        wire_record(
            &raw,
            &row.record,
            invocation.started,
            realtime()?,
            filename,
            &source,
            false,
            commit,
            "clean",
        )?;
        packets.push(raw);
    }
    Ok(packets)
}
fn compare_native(
    corpus: &Corpus,
    inputs: &Inputs,
    binary: &Path,
    root: &Path,
    output: &Path,
    commit: &str,
) -> Result<Value> {
    let env = config();
    let mut bridges = Vec::new();
    for (i, mode) in console_modes().iter().enumerate() {
        let env = console_environment(&env, i, *mode);
        let (peer, packets) = console_case(
            binary,
            root,
            &output.join(format!("console-{i}/rust")),
            &env,
            false,
        )?;
        fresh_peer(&peer, inputs, root, commit)?;
        equal_packets(&project(&corpus.console[i].packets)?, &packets, false)?;
        compare_responses(&corpus.console[i], &peer.responses, false)?;
        // Use existing console() for the fresh projection too; bridge() retains
        // raw identities and uses exactly that declared replacement operation.
        let fresh_raw = fs::read(peer.output.join("stdout.txt"))?;
        let filename = peer
            .packets
            .first()
            .and_then(|p| p["record"]["filename"].as_str())
            .ok_or("fresh console callsite missing")?;
        let comparison = bridge(
            &corpus.console[i].stdout,
            &corpus.console[i].filename,
            &fresh_raw,
            filename,
        )?;
        require(
            console(&peer)?
                == String::from_utf8(corpus.console[i].stdout.clone())?
                    .replace(&format!("{}: ", corpus.console[i].filename), "FILE: ")
                    .into_bytes(),
            "fresh console comparator differs",
        )?;
        bridges.push(comparison);
    }
    let (rate, packets) = rate_case(binary, root, &output.join("rate/rust"), &env, false)?;
    fresh_peer(&rate, inputs, root, commit)?;
    equal_packets(&project(&corpus.rate.packets)?, &packets, false)?;
    compare_responses(&corpus.rate, &rate.responses, true)?;
    let transport = transport::check_native(binary, root, &output.join("transport"))?;
    let binary_sha256 = digest(binary)?;
    let native_dir = output.join("transport/rust");
    let threads = fresh_transport(
        inputs,
        &native_dir,
        "threads",
        root,
        commit,
        binary,
        &binary_sha256,
    )?;
    equal_packets(
        &project(&corpus.threads.packets)?,
        &project(&threads)?,
        true,
    )?;
    let failure = fresh_transport(
        inputs,
        &native_dir,
        "console-failure",
        root,
        commit,
        binary,
        &binary_sha256,
    )?;
    equal_packets(
        &project(&corpus.failure.packets)?,
        &project(&failure)?,
        false,
    )?;
    let runtime = fresh_transport(
        inputs,
        &output.join("transport"),
        "runtime-prefix",
        root,
        commit,
        binary,
        &binary_sha256,
    )?;
    require(runtime.len() == 1, "fresh runtime packet count differs")?;
    let mut fresh_responses = 0;
    for case in (0..7)
        .map(|i| output.join(format!("console-{i}/rust")))
        .chain([
            output.join("rate/rust"),
            native_dir.join("threads"),
            native_dir.join("backpressure"),
            native_dir.join("console-failure"),
            output.join("transport/runtime-prefix"),
        ])
    {
        let rows: Vec<Value> = serde_json::from_slice(&fs::read(case.join("responses.json"))?)?;
        fresh_responses += rows.len();
    }
    let fresh_packets = 7 * 24 + rate.packets.len() + threads.len() + failure.len() + runtime.len();
    require(
        (fresh_packets, fresh_responses) == (250, 697),
        "fresh native cohort incomplete",
    )?;
    Ok(
        json!({"wire_records":168,"logprint_cases":7,"rate_inputs":516,"rate_records":16,"retained_cpp_packets":249,"retained_cpp_responses":694,"fresh_native_packets":fresh_packets,"fresh_native_responses":fresh_responses,"historical_console_modes":corpus.old_console.len(),"historical_to_recovered_console":corpus.bridges,"recovered_to_fresh_console":bridges,"recorded_controller_finished":corpus.end,"historical_timestamp_upper_bound":"authenticated artifact publication time; per-peer end unavailable","transport":transport}),
    )
}

#[cfg(test)]
#[path = "retained_tests.rs"]
mod tests;

#[path = "prepare.rs"]
pub(super) mod prepare;
