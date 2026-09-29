use openpilot_cereal::log_capnp::event;
use openpilot_proclogd::wire::encode_snapshot;
use openpilot_runtime_core::{
    proc_stat::ProcessStat,
    procfs::{CpuTimes, Memory, Process, ProportionalMemory, Snapshot},
};
use std::num::NonZeroU64;

fn snapshot() -> Snapshot {
    Snapshot {
        ticks_per_second: NonZeroU64::new(100).unwrap(),
        page_size: NonZeroU64::new(4096).unwrap(),
        cpu_times: vec![CpuTimes {
            cpu: 7,
            ticks: [100, 2, 30, 400, 5, 6, 7],
        }],
        memory: Memory {
            total: 131072,
            free: 16384,
            available: 32768,
            buffers: 2048,
            cached: 4096,
            active: 5120,
            inactive: 6144,
            shared: 7168,
        },
        processes: vec![Process {
            stat: ProcessStat {
                pid: 123,
                name: "worker ) (a".into(),
                state: b'S',
                ppid: 1,
                user_ticks: 125,
                system_ticks: 25,
                children_user_ticks: -3,
                children_system_ticks: 4,
                priority: 20,
                nice: -5,
                threads: 3,
                start_ticks: 500,
                virtual_bytes: 16777216,
                rss_pages: 2000,
                processor: 6,
            },
            resident_bytes: 8192000,
            exe: "/synthetic/bin/worker".into(),
            cmdline: vec!["worker".into(), "--test".into()],
            proportional: ProportionalMemory {
                pss: 51200,
                anon: 30720,
                shared: 10240,
            },
        }],
        warnings: vec![],
    }
}

#[test]
fn roundtrip_preserves_canonical_event_fields_and_signed_process_values() {
    let sample = snapshot();

    let bytes = encode_snapshot(&sample, 123456789).unwrap();

    let reader = capnp::serialize::read_message(bytes.as_slice(), Default::default()).unwrap();
    let event = reader.get_root::<event::Reader>().unwrap();
    assert_eq!(event.get_log_mono_time(), 123456789);
    assert!(event.get_valid());
    let event::ProcLog(log) = event.which().unwrap() else {
        panic!("not procLog")
    };
    let log = log.unwrap();
    assert_eq!(log.get_mem().unwrap().get_total(), 131072);
    assert_eq!(log.get_mem().unwrap().get_shared(), 7168);
    let cpus = log.get_cpu_times().unwrap();
    assert_eq!(cpus.len(), 1);
    assert_eq!(cpus.get(0).get_cpu_num(), 7);
    assert!((cpus.get(0).get_idle() - 4.0).abs() < f32::EPSILON);
    let p = log.get_procs().unwrap().get(0);
    assert_eq!(p.get_pid(), 123);
    assert_eq!(p.get_state(), b'S');
    assert_eq!(p.get_name().unwrap().to_str().unwrap(), "worker ) (a");
    assert!((p.get_cpu_user() - 1.25).abs() < f32::EPSILON);
    assert!((p.get_cpu_children_user() + 0.03).abs() < f32::EPSILON);
    assert!((p.get_start_time() - 5.0).abs() < f64::EPSILON);
    assert_eq!(p.get_nice(), -5);
    assert_eq!(p.get_mem_rss(), 8192000);
    assert_eq!(p.get_mem_pss(), 51200);
    assert_eq!(
        p.get_cmdline().unwrap().get(1).unwrap().to_str().unwrap(),
        "--test"
    );
}

#[test]
fn empty_snapshot_is_a_valid_empty_proclog() {
    let mut sample = snapshot();
    sample.processes.clear();
    sample.cpu_times.clear();

    let bytes = encode_snapshot(&sample, 1).unwrap();

    let reader = capnp::serialize::read_message(bytes.as_slice(), Default::default()).unwrap();
    let event = reader.get_root::<event::Reader>().unwrap();
    let event::ProcLog(log) = event.which().unwrap() else {
        panic!("not procLog")
    };
    assert!(log.unwrap().get_procs().unwrap().is_empty());
}

#[test]
fn out_of_schema_integer_ranges_return_an_error_without_wrapping() {
    let mut sample = snapshot();
    sample.processes[0].stat.pid = u32::MAX;

    let result = encode_snapshot(&sample, 1);

    assert!(result.is_err());
}
