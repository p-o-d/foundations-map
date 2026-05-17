//! Stage 1: gzip producer.
//!
//! Spawns a thread that decompresses the save file and sends 64 KB chunks
//! through a bounded `mpsc::SyncSender`. The caller drains them into the
//! Stage 2 byte scanner.

// Implementation in Task 3.
