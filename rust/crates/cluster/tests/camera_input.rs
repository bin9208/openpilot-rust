use openpilot_cluster::camera::input::Input;
use openpilot_msgq::{VisionClient, VisionLayout, VisionMetadata, VisionServer, VisionStream};

#[test]
fn wide_switch_waits_for_a_frame_and_stale_stream_reconnects() {
    let name = format!("cluster-camera-{}", std::process::id());
    let server = VisionServer::new(&name).unwrap();
    let layout = VisionLayout {
        width: 8,
        height: 4,
        stride: 16,
        uv_offset: 64,
        len: 96,
    };
    let road = server.create_stream(VisionStream::Road, 2, layout).unwrap();
    let wide = server
        .create_stream(VisionStream::WideRoad, 2, layout)
        .unwrap();
    server.start_listener().unwrap();
    let publish = |image: &openpilot_msgq::VisionImage, id: u32, byte: u8| {
        image.write(0, &[byte; 96]).unwrap();
        image
            .publish(VisionMetadata {
                width: 8,
                height: 4,
                stride: 16,
                uv_offset: 64,
                len: 96,
                frame_id: id,
                timestamp_sof: u64::from(id) * 1000,
                timestamp_eof: u64::from(id) * 1000 + 100,
                valid: false,
                received: true,
                index: 0,
                fd: -1,
            })
            .unwrap();
    };
    let mut input = Input::for_server(&name).unwrap();
    assert!(!input.select_stream(true, 1.0).unwrap());
    input.poll(1.0).unwrap();
    publish(&road[0], 41, 73);
    input.poll(1.125).unwrap();
    assert_eq!(input.frame().unwrap().metadata.frame_id, 41);
    assert_eq!(input.frame().unwrap().bytes, [73; 96]);
    assert!(!input.select_stream(true, 1.125).unwrap());
    publish(&wide[0], 42, 123);
    assert!(input.select_stream(true, 1.25).unwrap());
    assert_eq!(input.frame().unwrap().metadata.frame_id, 42);
    assert_eq!(input.frame().unwrap().bytes, [123; 96]);
    input.poll(2.375).unwrap();
    assert!(input.frame().is_some());
    input.poll(2.5).unwrap();
    assert!(input.frame().is_none());
    input.poll(2.625).unwrap();
    publish(&wide[1], 43, 200);
    input.poll(2.75).unwrap();
    assert_eq!(input.frame().unwrap().metadata.frame_id, 43);
    input.close();
    assert!(input.frame().is_none());
    assert!(input.available_streams().is_empty());
}

#[test]
fn wide_only_server_keeps_the_missing_road_stream_waiting() {
    let name = format!("cluster-wide-{}", std::process::id());
    let server = VisionServer::new(&name).unwrap();
    let _images = server
        .create_stream(
            VisionStream::WideRoad,
            2,
            VisionLayout {
                width: 8,
                height: 4,
                stride: 16,
                uv_offset: 64,
                len: 96,
            },
        )
        .unwrap();
    server.start_listener().unwrap();
    let mut input = Input::for_server(&name).unwrap();
    assert!(!input.select_stream(false, 1.0).unwrap());
    input.poll(1.0).unwrap();
    assert!(input.frame().is_none());
}

#[test]
fn retained_camera_buffer_keeps_its_validated_mapping_and_live_bytes() {
    let name = format!("cluster-retained-{}", std::process::id());
    let server = VisionServer::new(&name).unwrap();
    let images = server
        .create_stream(
            VisionStream::Road,
            2,
            VisionLayout {
                width: 8,
                height: 4,
                stride: 16,
                uv_offset: 64,
                len: 96,
            },
        )
        .unwrap();
    server.start_listener().unwrap();
    let mut client = VisionClient::new(&name, VisionStream::Road, true).unwrap();
    assert!(client.connect().unwrap());
    let retained = client.retained_buffer(0).unwrap();
    images[0].write(0, &[72; 96]).unwrap();
    let mut bytes = [0; 96];
    retained.copy_into(&mut bytes).unwrap();
    assert_eq!(bytes, [72; 96]);
    images[0].write(0, &[92; 96]).unwrap();
    retained.copy_into(&mut bytes).unwrap();
    assert_eq!(bytes, [92; 96]);
    assert_eq!(retained.layout().uv_offset, 64);
    assert!(client.retained_buffer(2).is_err());
    drop(images);
    drop(server);
    retained.copy_into(&mut bytes).unwrap();
    assert_eq!(bytes, [92; 96]);
}
