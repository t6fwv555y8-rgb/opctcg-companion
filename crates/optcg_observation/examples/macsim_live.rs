//! Watches the OPTCGSim window and prints what the reader understands.
//!
//!     cargo run -p optcg_observation --example macsim_live

use optcg_observation::macsim::SimReader;

fn main() {
    let mut reader = SimReader::new();
    let rounds: u32 = std::env::args().nth(1).and_then(|n| n.parse().ok()).unwrap_or(5);
    for round in 0..rounds {
        let started = std::time::Instant::now();
        let poll = reader.poll();
        println!("--- poll {} ({:.1}s): {}", round + 1, started.elapsed().as_secs_f32(), poll.status.message());
        for m in &poll.new_messages {
            println!("  log: {m}");
        }
        if let Some(s) = &poll.snapshot {
            let leader = |p: &Option<optcg_observation::bridge_protocol::BrowserPlayerSnapshot>| {
                p.as_ref().and_then(|p| p.leader_id.clone()).unwrap_or_else(|| "?".into())
            };
            println!(
                "  you {} vs {} · turn {:?} · active {:?} · combat {} · over {:?}/{:?}",
                leader(&s.self_player), leader(&s.opponent), s.turn, s.active_player,
                s.combat.is_some(), s.game_over, s.result
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(800));
    }
}
