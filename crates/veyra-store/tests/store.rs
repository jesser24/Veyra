use std::fs;
use std::path::PathBuf;
use veyra_store::crypto::KdfParams;
use veyra_store::db::{Store, View};
use veyra_store::{backup, vault, Error};

fn tmp(name: &str) -> PathBuf { let d = std::env::temp_dir().join(format!("veyra-{name}-{}-{}", std::process::id(), veyra_store::crypto::hex(&veyra_store::crypto::random::<4>()))); fs::create_dir_all(&d).unwrap(); d }
const PW: &str = "correct horse battery";
fn setup(name: &str) -> (PathBuf, Store, String) {
    let d = tmp(name);
    let (u, rk) = vault::create(&d.join("vault.json"), PW, KdfParams::TEST).unwrap();
    (d.clone(), Store::open(&d, &u).unwrap(), rk)
}

#[test] fn vault_unlocks_with_password_and_recovery_but_not_with_a_wrong_secret() {
    let d = tmp("vault"); let p = d.join("vault.json");
    let (u, rk) = vault::create(&p, PW, KdfParams::TEST).unwrap();
    assert_eq!(vault::unlock(&p, PW).unwrap().db_key().0, u.db_key().0);
    assert_eq!(vault::unlock_with_recovery(&p, &rk).unwrap().db_key().0, u.db_key().0);
    assert!(matches!(vault::unlock(&p, "wrong password!"), Err(Error::WrongSecret)));
    assert!(matches!(vault::unlock_with_recovery(&p, "0000"), Err(Error::WrongSecret)));
    assert!(vault::create(&p, PW, KdfParams::TEST).is_err(), "never overwrite an existing vault");
    assert!(vault::create(&d.join("x.json"), "short", KdfParams::TEST).is_err());
}
#[test] fn changing_the_password_keeps_data_readable_and_invalidates_the_old_one() {
    let (d, s, rk) = setup("chpw");
    let c = s.create_conversation("hello", 1).unwrap(); s.add_message(&c.id, "user", "secret text", "sent", 2).unwrap(); drop(s);
    let p = d.join("vault.json");
    vault::change_password(&p, PW, "a brand new password").unwrap();
    assert!(vault::unlock(&p, PW).is_err()); assert!(vault::change_password(&p, "nope nope nope", "another password").is_err());
    let s = Store::open(&d, &vault::unlock(&p, "a brand new password").unwrap()).unwrap();
    assert_eq!(s.messages(&c.id).unwrap()[0].content, "secret text");
    drop(s); vault::reset_password_with_recovery(&p, &rk, "reset via recovery").unwrap();
    assert!(Store::open(&d, &vault::unlock(&p, "reset via recovery").unwrap()).is_ok());
}
#[test] fn database_file_contains_no_plaintext_and_cannot_be_opened_with_another_key() {
    let (d, s, _) = setup("enc");
    let c = s.create_conversation("UNIQUE-TITLE-XYZ", 1).unwrap(); s.add_message(&c.id, "user", "UNIQUE-BODY-QRS", "sent", 2).unwrap(); drop(s);
    let raw = fs::read(Store::db_path(&d)).unwrap();
    assert!(!raw.windows(15).any(|w| w == b"UNIQUE-TITLE-XY") && !raw.windows(14).any(|w| w == b"UNIQUE-BODY-QR"), "plaintext found in the database file");
    assert!(!raw.starts_with(b"SQLite format 3"), "database header is not encrypted");
    let other = tmp("other"); let (u2, _) = vault::create(&other.join("vault.json"), PW, KdfParams::TEST).unwrap();
    assert!(matches!(Store::open(&d, &u2), Err(Error::WrongSecret)));
}
#[test] fn conversations_folders_favorites_archive_search_and_trash_lifecycle() {
    let (_d, s, _) = setup("conv");
    let a = s.create_conversation("Rust lifetimes", 10).unwrap(); let b = s.create_conversation("Recipes", 20).unwrap();
    s.add_message(&a.id, "user", "What is 100% of a borrow_checker?", "sent", 30).unwrap();
    s.rename(&b.id, "  Dinner ideas ").unwrap(); assert_eq!(s.conversation(&b.id).unwrap().title, "Dinner ideas");
    assert_eq!(s.list(View::Active, None).unwrap()[0].id, a.id, "most recently updated first");
    assert_eq!(s.search("BORROW").unwrap().len(), 1); assert_eq!(s.search("100%").unwrap().len(), 1); assert_eq!(s.search("%").unwrap().len(), 1, "% is literal"); assert_eq!(s.search("_").unwrap().len(), 1, "_ is literal");
    assert_eq!(s.search("lifetimes").unwrap()[0].id, a.id);
    s.set_favorite(&a.id, true).unwrap(); assert_eq!(s.list(View::Favorites, None).unwrap().len(), 1);
    let f = s.create_folder("Work", 1).unwrap(); s.move_to_folder(&b.id, Some(&f.id)).unwrap(); assert_eq!(s.list(View::Active, Some(&f.id)).unwrap().len(), 1);
    s.set_archived(&b.id, true).unwrap(); assert_eq!(s.list(View::Archived, None).unwrap().len(), 1); assert_eq!(s.list(View::Active, None).unwrap().len(), 1);
    s.trash(&a.id, 100).unwrap(); assert_eq!(s.list(View::Trash, None).unwrap().len(), 1); assert!(s.search("borrow").unwrap().is_empty(), "trashed conversations are not searched");
    s.restore(&a.id).unwrap(); assert_eq!(s.list(View::Trash, None).unwrap().len(), 0);
    s.trash(&a.id, 100).unwrap(); assert_eq!(s.purge_trash_older_than(50).unwrap(), 0); assert_eq!(s.purge_trash_older_than(101).unwrap(), 1);
    assert!(matches!(s.conversation(&a.id), Err(Error::NotFound(_)))); assert!(s.rename("nope", "x").is_err());
}
#[test] fn unsent_messages_survive_and_can_be_listed_for_retry() {
    let (d, s, _) = setup("pending");
    let c = s.create_conversation("t", 1).unwrap(); let m = s.add_message(&c.id, "user", "typed while offline", "pending", 2).unwrap(); drop(s);
    let s = Store::open(&d, &vault::unlock(&d.join("vault.json"), PW).unwrap()).unwrap();
    assert_eq!(s.pending_messages().unwrap().len(), 1);
    s.set_message(&m.id, "typed while offline", "sent").unwrap(); assert!(s.pending_messages().unwrap().is_empty());
}
#[test] fn attachments_are_encrypted_verified_and_deleted_with_the_conversation() {
    let (d, s, _) = setup("att");
    let c = s.create_conversation("t", 1).unwrap();
    let data = b"%PDF-1.7 PLAINTEXT-MARKER-12345 pretend document".to_vec();
    let a = s.add_attachment(&c.id, "doc.pdf", "application/pdf", &data, 2).unwrap();
    let blob = Store::attachments_dir(&d).join(format!("{}.bin", a.id)); let raw = fs::read(&blob).unwrap();
    assert!(!raw.windows(16).any(|w| w == b"PLAINTEXT-MARKER"), "attachment stored in clear");
    assert_eq!(s.read_attachment(&a.id).unwrap(), data); assert_eq!(s.attachments(&c.id).unwrap()[0].size, data.len() as i64);
    let mut bad = raw.clone(); let n = bad.len(); bad[n - 1] ^= 1; fs::write(&blob, bad).unwrap();
    assert!(matches!(s.read_attachment(&a.id), Err(Error::Integrity(_))), "tampering must be detected");
    fs::write(&blob, raw).unwrap();
    s.trash(&c.id, 3).unwrap(); assert!(blob.exists(), "trash keeps files so restore works");
    s.delete_permanently(&c.id).unwrap(); assert!(!blob.exists(), "permanent deletion must remove the attachment file");
}
#[test] fn backup_verify_restore_on_another_directory_with_password_or_recovery_key() {
    let (d, s, rk) = setup("bk");
    let c = s.create_conversation("kept", 1).unwrap(); s.add_message(&c.id, "user", "important", "sent", 2).unwrap();
    let a = s.add_attachment(&c.id, "n.txt", "text/plain", b"attachment body", 3).unwrap();
    let bdir = tmp("backups"); let b = backup::create(&s, &d, &bdir, 1000).unwrap();
    backup::verify(&b).unwrap();
    s.add_message(&c.id, "user", "written after the backup", "sent", 4).unwrap();
    let t1 = tmp("restore1"); fs::remove_dir_all(&t1).unwrap();
    backup::restore(&b, &t1, Some(PW), None).unwrap();
    let r = Store::open(&t1, &vault::unlock(&t1.join("vault.json"), PW).unwrap()).unwrap();
    assert_eq!(r.messages(&c.id).unwrap().len(), 1); assert_eq!(r.read_attachment(&a.id).unwrap(), b"attachment body");
    let t2 = tmp("restore2"); fs::remove_dir_all(&t2).unwrap();
    backup::restore(&b, &t2, None, Some(&rk)).unwrap();
    let t3 = tmp("restore3"); fs::remove_dir_all(&t3).unwrap();
    assert!(backup::restore(&b, &t3, Some("wrong password"), None).is_err()); assert!(!t3.exists() || fs::read_dir(&t3).unwrap().next().is_none());
    assert!(backup::restore(&b, &d, Some(PW), None).is_err(), "never restore over existing data");
    let mut raw = fs::read(b.join("veyra.db")).unwrap(); raw[100] ^= 0xff; fs::write(b.join("veyra.db"), raw).unwrap();
    assert!(matches!(backup::verify(&b), Err(Error::Integrity(_))), "a damaged backup is detected before restoring");
}
#[test] fn backup_retention_by_age_count_and_size_keeps_the_newest() {
    let (d, s, _) = setup("ret"); let bdir = tmp("retb"); let day = 86_400_000i64;
    for i in 0..5 { backup::create(&s, &d, &bdir, 1_000_000 + i * day).unwrap(); }
    let now = 1_000_000 + 4 * day;
    assert_eq!(backup::prune(&bdir, now, 30, 3, u64::MAX).unwrap().len(), 2, "count limit");
    assert_eq!(backup::prune(&bdir, now + 10 * day, 11, 10, u64::MAX).unwrap().len(), 1, "age limit: only the day-2 backup is older than 11 days");
    let left = fs::read_dir(&bdir).unwrap().count();
    backup::prune(&bdir, now, 30, 10, 1).unwrap();
    assert_eq!(fs::read_dir(&bdir).unwrap().count(), 1, "size limit trims to the newest only, never to zero (had {left})");
}
