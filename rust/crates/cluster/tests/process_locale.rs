use openpilot_cluster::runtime_locale::select;

#[test]
fn source_candidate_order_stops_at_first_available_locale() {
    for accepted in [Some("C.UTF-8"), Some("C"), None] {
        let mut attempted = Vec::new();
        let selected = select(|candidate| {
            attempted.push(candidate);
            candidate == accepted.unwrap_or("")
        });
        assert_eq!(selected, accepted);
        assert_eq!(
            attempted,
            if accepted == Some("C.UTF-8") {
                vec!["C.UTF-8"]
            } else {
                vec!["C.UTF-8", "C"]
            }
        );
        println!("accepted {accepted:?}, attempted {attempted:?}");
    }
}
