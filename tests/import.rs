mod common;

use common::{assert_niente_tmp, immagine, importa, pacchetto_finto};
use predicates::prelude::*;
use tempfile::tempdir;

#[test]
fn import_base_produce_un_kpackage_valido() {
    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("share/wallpapers");
    let sistema = tmp.path().join("sistema");
    std::fs::create_dir_all(&sistema).unwrap();
    let src = immagine(
        tmp.path(),
        "Tramonto Cala Luna.png",
        1600,
        900,
        [10, 20, 30],
    );

    importa(&dest, &sistema, &[src.to_str().unwrap()])
        .code(0)
        .stdout(predicate::str::contains("importato: Tramonto_Cala_Luna"))
        .stdout(predicate::str::contains("1 importati"))
        .stderr(predicate::str::is_empty());

    let pkg = dest.join("Tramonto_Cala_Luna");
    assert!(pkg.join("metadata.json").is_file());
    assert!(pkg.join("contents/screenshot.png").is_file());
    let originale = pkg.join("contents/images/1600x900.png");
    assert_eq!(
        std::fs::read(&originale).unwrap(),
        std::fs::read(&src).unwrap()
    );

    let meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(pkg.join("metadata.json")).unwrap()).unwrap();
    assert_eq!(meta["KPlugin"]["Id"], "Tramonto_Cala_Luna");
    assert_eq!(meta["KPlugin"]["Name"], "Tramonto Cala Luna");

    assert_niente_tmp(&dest);
}

#[test]
fn stesso_file_due_volte_non_duplica() {
    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("share/wallpapers");
    let sistema = tmp.path().join("sistema");
    std::fs::create_dir_all(&sistema).unwrap();
    let src = immagine(tmp.path(), "foresta.png", 1600, 900, [1, 2, 3]);

    importa(&dest, &sistema, &[src.to_str().unwrap()]).success();

    importa(&dest, &sistema, &[src.to_str().unwrap()])
        .code(0)
        .stdout(predicate::str::contains("duplicato di: foresta"))
        .stdout(predicate::str::contains("0 importati, 1 duplicati"));

    assert!(!dest.join("foresta-2").exists());
    assert_niente_tmp(&dest);
}

#[test]
fn file_diverso_con_lo_stesso_nome_prende_il_suffisso() {
    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("share/wallpapers");
    let sistema = tmp.path().join("sistema");
    std::fs::create_dir_all(&sistema).unwrap();

    let a = immagine(tmp.path(), "foresta.png", 1600, 900, [1, 2, 3]);
    let sub = tmp.path().join("altra");
    std::fs::create_dir_all(&sub).unwrap();
    let b = immagine(&sub, "foresta.png", 1600, 900, [9, 9, 9]);

    importa(&dest, &sistema, &[a.to_str().unwrap()]).success();
    importa(&dest, &sistema, &[b.to_str().unwrap()])
        .code(0)
        .stdout(predicate::str::contains("importato: foresta-2"));

    assert!(dest.join("foresta").is_dir());
    assert!(dest.join("foresta-2").is_dir());
    assert_niente_tmp(&dest);
}

#[test]
fn non_maschera_un_wallpaper_di_sistema() {
    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("share/wallpapers");
    let sistema = tmp.path().join("sistema");
    std::fs::create_dir_all(sistema.join("wallpapers")).unwrap();
    pacchetto_finto(&sistema.join("wallpapers"), "Altai");

    let src = immagine(tmp.path(), "Altai.png", 1600, 900, [4, 5, 6]);
    importa(&dest, &sistema, &[src.to_str().unwrap()])
        .code(0)
        .stdout(predicate::str::contains("importato: Altai-2"));

    assert!(dest.join("Altai-2").is_dir());
    assert!(
        !dest.join("Altai").exists(),
        "il nome di sistema resta libero"
    );
    assert_niente_tmp(&dest);
}

#[test]
fn la_catena_dei_suffissi_riusa_i_buchi() {
    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("share/wallpapers");
    let sistema = tmp.path().join("sistema");
    std::fs::create_dir_all(&sistema).unwrap();

    for (i, tinta) in [[1, 1, 1], [2, 2, 2], [3, 3, 3]].iter().enumerate() {
        let sub = tmp.path().join(format!("d{i}"));
        std::fs::create_dir_all(&sub).unwrap();
        let f = immagine(&sub, "mare.png", 1600, 900, *tinta);
        importa(&dest, &sistema, &[f.to_str().unwrap()]).success();
    }
    assert!(dest.join("mare-3").is_dir());

    std::fs::remove_dir_all(dest.join("mare-2")).unwrap();
    let sub = tmp.path().join("d9");
    std::fs::create_dir_all(&sub).unwrap();
    let f = immagine(&sub, "mare.png", 1600, 900, [7, 7, 7]);
    importa(&dest, &sistema, &[f.to_str().unwrap()])
        .code(0)
        .stdout(predicate::str::contains("importato: mare-2"));

    assert!(dest.join("mare-2").is_dir(), "il buco viene riusato");
    assert_niente_tmp(&dest);
}

#[test]
fn file_corrotto_e_illeggibile_falliscono_senza_pacchetti_parziali() {
    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("share/wallpapers");
    let sistema = tmp.path().join("sistema");
    std::fs::create_dir_all(&sistema).unwrap();

    let corrotto = tmp.path().join("rotto.png");
    let buono = immagine(tmp.path(), "buono.png", 1600, 900, [1, 2, 3]);
    let bytes = std::fs::read(&buono).unwrap();
    std::fs::write(&corrotto, &bytes[..12]).unwrap();

    let assente = tmp.path().join("mai-esistito.png");

    importa(
        &dest,
        &sistema,
        &[corrotto.to_str().unwrap(), assente.to_str().unwrap()],
    )
    .code(2)
    .stdout(predicate::str::contains("0 importati, 2 errori"))
    .stderr(predicate::str::contains("danneggiata"))
    .stderr(predicate::str::contains("impossibile leggere il file"));

    assert!(!dest.join("rotto").exists());
    assert_niente_tmp(&dest);
}

#[test]
fn selezione_mista_restituisce_successo_parziale() {
    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("share/wallpapers");
    let sistema = tmp.path().join("sistema");
    std::fs::create_dir_all(&sistema).unwrap();

    let buono = immagine(tmp.path(), "buono.png", 1600, 900, [1, 2, 3]);
    importa(&dest, &sistema, &[buono.to_str().unwrap()]).success();

    let corrotto = tmp.path().join("rotto.png");
    std::fs::write(&corrotto, b"non sono un png").unwrap();
    let altro = immagine(tmp.path(), "altro.png", 1600, 900, [4, 4, 4]);

    importa(
        &dest,
        &sistema,
        &[
            altro.to_str().unwrap(),
            buono.to_str().unwrap(),
            corrotto.to_str().unwrap(),
        ],
    )
    .code(1)
    .stdout(predicate::str::contains(
        "1 importati, 1 duplicati, 1 errori",
    ));

    assert_niente_tmp(&dest);
}

#[test]
fn immagine_piccola_passa_con_force() {
    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("share/wallpapers");
    let sistema = tmp.path().join("sistema");
    std::fs::create_dir_all(&sistema).unwrap();
    let piccola = immagine(tmp.path(), "icona.png", 64, 64, [1, 2, 3]);

    importa(&dest, &sistema, &["--force", piccola.to_str().unwrap()])
        .code(0)
        .stdout(predicate::str::contains("importato: icona"));

    assert!(dest.join("icona").is_dir());
    assert_niente_tmp(&dest);
}

#[test]
fn argomenti_mancanti_danno_exit_64() {
    assert_cmd::Command::cargo_bin("kde-wallpaper-import")
        .unwrap()
        .assert()
        .code(64)
        .stderr(predicate::str::contains("FILE"));
}
