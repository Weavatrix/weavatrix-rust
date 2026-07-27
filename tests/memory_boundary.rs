#[cfg(feature = "memory")]
#[test]
fn rust_core_exposes_the_standalone_memory_module() {
    use weavatrix_rust::memory::{EntityId, Timestamp};

    let entity = EntityId::new("task:memory-boundary").unwrap();
    let timestamp = Timestamp::from_unix_micros(42);

    assert_eq!(entity.as_str(), "task:memory-boundary");
    assert_eq!(timestamp.as_unix_micros(), 42);
}
