//! Cursor de paginación del historial.
//!
//! `git log --skip=N` recorre los N commits saltados: el coste de una página crece con la
//! profundidad (481 ms en la última página de 100 000 commits, `specs/rust/rendimiento.md`).
//! El cursor lo evita: la página siguiente se pide como `git log <frontera>`, que empieza justo
//! donde acabó la anterior y cuesta lo mismo en cualquier profundidad.
//!
//! **No basta con el padre del último commit.** En un historial con merges, la cola de
//! recorrido de git contiene varios commits pendientes (uno por rama aún sin terminar);
//! `git log <último>^@` los perdería. La *frontera* es el conjunto de commits que son padres de
//! algo ya mostrado y no se han mostrado todavía, en el orden en que git los fue encolando.
//! Empezar un recorrido nuevo desde esa frontera reproduce exactamente el resto del recorrido
//! (mismo orden, mismo contenido); en un historial lineal es solo el padre del último commit.

//!
//! # Repetidos (fechas iguales)
//!
//! Git puede emitir un commit *antes* que un descendiente suyo cuando ambos tienen la misma
//! fecha (varios commits en el mismo segundo, `fast-import`, scripts). Entonces un recorrido
//! nuevo desde la frontera vuelve a alcanzar ese commit ya mostrado. Es inofensivo para el
//! resto: **los commits que aún no se habían mostrado salen en el mismo orden que con
//! `--skip`**, porque lo repetido solo añade commits ya vistos a la cola. Quien consume el
//! cursor descarta los que ya tiene ([`append_new`]). Si una página entera son repetidos no hay
//! avance posible con ese cursor y se pide esa página con `--skip` (siempre exacto).

use std::collections::HashSet;

use crate::models::Commit;

/// Añade a `shown` los commits de `page` que aún no estaban (conserva el orden) y devuelve
/// cuántos eran nuevos. `0` con la frontera aún sin agotar = página de repetidos: pedir esa
/// página por `--skip`.
pub fn append_new(shown: &mut Vec<Commit>, page: Vec<Commit>) -> usize {
    let have: HashSet<String> = shown.iter().map(|c| c.sha.clone()).collect();
    let before = shown.len();
    let mut added: HashSet<String> = HashSet::new();
    for c in page {
        if !have.contains(&c.sha) && added.insert(c.sha.clone()) {
            shown.push(c);
        }
    }
    shown.len() - before
}

/// Frontera tras haber mostrado `shown` (en el orden de `git log`). Vacía = no queda historial.
///
/// Es la función de referencia; la UI usa un equivalente incremental
/// (`ui/src/lib/util/cursor.ts`) y los tests comprueban que ambas coinciden.
pub fn history_cursor(shown: &[Commit]) -> Vec<String> {
    let loaded: HashSet<&str> = shown.iter().map(|c| c.sha.as_str()).collect();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut out = Vec::new();
    for c in shown {
        for p in &c.parents {
            if !loaded.contains(p.as_str()) && seen.insert(p.as_str()) {
                out.push(p.clone());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(sha: &str, parents: &[&str]) -> Commit {
        Commit {
            sha: sha.into(),
            parents: parents.iter().map(|p| (*p).to_owned()).collect(),
            subject: String::new(),
            body: String::new(),
            author_name: String::new(),
            author_email: String::new(),
            author_date_iso: String::new(),
            refs: Vec::new(),
        }
    }

    #[test]
    fn linear_history_is_the_parent_of_the_last() {
        let shown = [c("c", &["b"]), c("b", &["a"])];
        assert_eq!(history_cursor(&shown), vec!["a"]);
    }

    #[test]
    fn merge_keeps_every_pending_branch_in_queue_order() {
        // m(p1=x, p2=y) → x(p=r) ; y ya cargado más tarde no cuenta.
        let shown = [c("m", &["x", "y"]), c("x", &["r"])];
        assert_eq!(history_cursor(&shown), vec!["y", "r"]);
        let shown = [c("m", &["x", "y"]), c("x", &["r"]), c("y", &["r"])];
        assert_eq!(history_cursor(&shown), vec!["r"]);
    }

    #[test]
    fn root_ends_the_history() {
        assert!(history_cursor(&[c("a", &[])]).is_empty());
        assert!(history_cursor(&[]).is_empty());
    }
}
