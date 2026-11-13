# Matriz de trazabilidad — requisitos → tarea → prueba real

Documento generado leyendo [`spec.md`](spec.md), [`tasks.md`](tasks.md) y **los nombres de
prueba que existen hoy** en `crates/`, `src-tauri/` y `ui/`. No es una lista de deseos: cada
celda de la columna «Prueba» es un `archivo:nombre_de_test` que está en el árbol y que
`cargo test` / `vitest` ejecutan. Lo que no tiene prueba lo dice.

- Fecha de generación: **2026-11-13**, rama `rust-migration`.
- Conteo (funciones con `#[test]`/`#[tokio::test]` en `crates/` y `src-tauri/`, y casos
  `it`/`test` en `ui/`): **760 pruebas de Rust** en 72 ficheros y **498 casos de UI** en 50
  ficheros de test.
- Cómo se regenera (desde la raíz del repo):

  ```sh
  grep -rn '#\[(tokio::)?test\]' crates src-tauri --include='*.rs' -A1 | grep fn
  grep -rnE "\b(it|test)\(\s*['\"\`]" ui/src --include='*.test.ts'
  ```

- Nombres de archivo abreviados en la tabla: `git/` = `crates/ariadna-git/tests/`,
  `core/` = `src-tauri/src/core/tests/`, `ui/` = `ui/src/lib/components/`.
- Marcas: **CUBIERTO** = hay al menos una prueba que ejerce el requisito tal como está
  escrito; **PARCIAL** = la prueba cubre una parte y el resto está en deuda; **HUECO** =
  ninguna prueba (y, si corresponde, tampoco implementación).

## 1. Resumen

| Familia | Requisitos | Cubiertos | Parciales | Huecos |
|---|---|---|---|---|
| Git (G-01…G-16) | 16 | 15 | 1 (G-16, cancelación de clonado en git 2.53) | 0 |
| Pull requests (PR-01…PR-10) | 10 | 8 | 1 | 1 (PR-09, refresco por intervalo) |
| IA (RF-IA1…RF-IA12) | 12 | 9 | 2 | 1 (RF-IA11, política administrable, posterior a v0.1) |
| No funcionales (NF-1…NF-8) | 8 | 3 | 2 | 3 (NF-1, NF-2, NF-6/NF-8 medidos a mano o sin prueba) |
| Contrato IPC, confirmaciones, pestañas (§3.3, §6, §8) | 3 bloques | 2 | 1 | 0 |

**Lectura:** el motor Git y la IA están cubiertos con pruebas reales (git real, CLI falso,
git falso para credenciales). Lo que queda flojo es lo que necesita **una ventana real**
(rendimiento de arranque, E2E con `tauri-driver`, regresión visual) y **un servidor real**
(Azure DevOps). Los huecos de PR-05 y PR-09 son deuda de producto, no de pruebas.

## 2. Git (G-01…G-16)

| ID | Tarea | Prueba (`archivo:test`) | Estado |
|---|---|---|---|
| G-01 abrir repo (sube a la raíz; error si no lo es) | M2 | `git/engine_core.rs:open_repository_from_subfolder_and_outside` · `core/tabs.rs:abre_un_repo_y_tambien_desde_una_subcarpeta` · `core/tabs.rs:una_carpeta_que_no_es_repo_da_not_a_repository` · `ui/Welcome.test.ts` | CUBIERTO |
| G-02 status porcelain v2 -z | M2 | `git/engine_core.rs:open_status_stage_commit_and_log` · `git/engine_core.rs:status_reports_ignored_only_when_asked` · `ariadna-git/src/porcelain.rs:ordinary_untracked_and_branch_headers`, `:rename_with_spaces_and_unicode`, `:rename_modified_and_conflict`, `:all_conflict_kinds`, `:detached_and_unborn`, `:ignored_untracked_and_submodule`, `:non_utf8_and_newline_paths_are_preserved`, `:garbage_and_truncated_input_never_panics` · `git/parser_props.rs:status_never_panics`, `:status_counts_match_generated_records` | CUBIERTO |
| G-03 stage/unstage por archivo | M2 | `git/engine_core.rs:unstage_works_with_and_without_head`, `:stage_handles_deletions`, `:stage_and_unstage_five_thousand_files`, `:paths_with_spaces_unicode_and_dash_are_literal`, `:glob_characters_in_names_are_not_expanded` · `git/bulk_paths.rs:bulk_stage_and_unstage_match_the_pathspec_route` | CUBIERTO |
| G-04 stage/unstage por hunk (CRLF, sin salto final, Unicode) | M2 | `git/engine_core.rs:stage_isolated_and_contiguous_hunks`, `:stage_single_hunk_and_unstage_it`, `:stage_hunk_with_crlf_under_every_autocrlf`, `:stage_hunk_with_missing_final_newline`, `:stage_hunk_in_unicode_path_with_spaces`, `:stage_hunk_in_dash_named_file`, `:stale_hunk_is_reported_as_not_found`, `:stage_hunk_of_untracked_file`, `:failed_untracked_hunk_leaves_no_intent_to_add`, `:stage_hunk_of_untracked_dash_file` · `ariadna-git/src/diff.rs:two_hunks_and_single_hunk_patch`, `:single_hunk_patch_applies_to_real_index_without_touching_cr` · `ui/DiffView.test.ts` | CUBIERTO |
| G-05 descartar con confirmación | M3 | `git/engine_core.rs:restore_worktree_discards_changes_but_leaves_untracked` · `core/destructive.rs:discard_sin_confirmacion_no_toca_nada`, `:discard_con_confirmacion_restaura_el_archivo` · `ui/ChangesPanel.test.ts:descartar llama al comando (la confirmación es nativa) y respeta la cancelación` | CUBIERTO |
| G-06 commit (mensaje obligatorio, aviso detached) | M2 | `git/engine_core.rs:empty_commit_message_and_nothing_staged`, `:commit_message_goes_through_stdin_verbatim`, `:detached_head_asks_before_commit` · `core/destructive.rs:commit_en_detached_con_y_sin_confirmacion`, `:commit_normal_y_errores_de_commit` · `ui/CommitBar.test.ts` | CUBIERTO |
| G-07 amend seguro | M2 | `git/engine_core.rs:amend_rewrites_unpublished_commit_only`, `:amend_refuses_when_another_branch_of_a_remote_contains_head`, `:amend_refuses_detached_head`, `:amend_refuses_during_merge_and_cherry_pick` · `ariadna-git/src/amend.rs:prefills_and_clears_untouched_message`, `:warns_on_merge_commit` · `core/destructive.rs:amend_draft_explica_por_que_no_se_puede` | CUBIERTO |
| G-08 log con autor/fecha/refs y diff del commit | M2 | `git/engine_core.rs:open_status_stage_commit_and_log`, `:log_is_empty_without_commits_and_respects_limit`, `:diff_scopes_commit_between_and_root`, `:diff_of_merge_commit_uses_first_parent`, `:diff_unstaged_staged_untracked_and_deleted`, `:diff_staged_works_in_unborn_repo_and_ignores_user_prefix_config`, `:diff_keeps_binary_and_crlf_bytes` · `ariadna-git/src/diff.rs:*` (15) · `core/clone_history.rs:el_diff_de_un_commit_da_sus_archivos` · `ui/HistoryPanel.test.ts` | CUBIERTO |
| G-09 filtro local y búsqueda global | M2 | `git/engine_core.rs:search_log_matches_message_author_and_hash`, `:search_by_hash_ignores_commits_on_other_branches`, `:search_in_unborn_repo_is_empty` · `core/clone_history.rs:log_page_pagina_sin_solapes_y_filtra_por_texto` · `ui/HistoryPanel.test.ts:buscar filtra y avisa cuando no hay resultados` | CUBIERTO |
| G-10 ramas (listar, crear, cambiar, renombrar, borrar, remotas) | M3 | `git/engine_branches.rs:branches_checkout_rename_and_delete`, `:create_branch_from_start_point_and_delete_unmerged_needs_force`, `:checkout_a_tag_leaves_head_detached`, `:checkout_dirty_tree_blocks_with_detail_and_keeps_changes`, `:checkout_allows_unrelated_local_changes`, `:checkout_blocks_when_untracked_file_would_be_overwritten`, `:checkout_remote_tracking_creates_local_branch_with_upstream`, `:checkout_remote_does_not_switch_diverged_local`, `:checkout_remote_with_local_following_another_upstream_is_diverged`, `:remote_tracking_branches_skip_origin_head`, `:invalid_references_are_rejected_before_running_git` · `core/clone_history.rs:borrar_rama_pide_confirmacion_nativa_y_respeta_el_no`, `:borrar_una_rama_sin_fusionar_pide_una_segunda_confirmacion` · `ui/BranchesPanel.test.ts` | CUBIERTO |
| G-11 fetch / pull (merge o rebase) / push con --set-upstream | M3 | `git/engine_branches.rs:push_set_upstream_then_branches_report_it`, `:push_without_refspec_on_detached_head_with_set_upstream_is_rejected`, `:fetch_updates_remote_tracking_branches_without_touching_local`, `:pull_fast_forwards_from_bare_remote`, `:pull_with_diverged_histories_creates_a_merge`, `:pull_rebase_replays_local_commit`, `:pull_rebase_conflict_abort_and_continue`, `:pull_merge_conflict_leaves_merge_in_progress` · `core/remotes.rs:fetch_pull_y_push_funcionan_contra_un_remoto_local` · `core/settings.rs:pull_sin_indicacion_usa_el_ajuste_de_rebase` · `git/auth_network.rs` (errores de red) | CUBIERTO |
| G-12 merge ff / no-ff / ff-only / ya al día | M3 | `git/engine_branches.rs:merge_fast_forward_no_ff_and_up_to_date`, `:merge_ff_only_that_cannot_advance_is_an_error_and_changes_nothing`, `:merge_ff_only_that_can_advance_works`, `:merge_blocked_by_local_changes_is_an_error_not_conflicts` · `core/clone_history.rs:merge_limpio_funciona_y_con_conflictos_aborta_con_error_tipado` | CUBIERTO |
| G-13 conflictos a tres vías y escritura con confirmación | M3/M5 | `git/engine_branches.rs:merge_conflict_returns_paths_then_resolve_mark_and_continue`, `:conflict_delete_modify_both_ways`, `:conflict_both_added`, `:conflict_rename_rename_gives_added_by_us_added_by_them_and_both_deleted`, `:conflict_binary_has_sides_but_no_hunks`, `:writing_a_resolution_preserves_crlf_and_bom`, `:write_creates_nested_files_without_staging_and_leaves_no_temp_files`, `:write_path_escape_does_not_write`, `:write_through_symlinked_directory_is_rejected`, `:write_over_symlinked_file_replaces_the_link_and_never_touches_the_target`, `:write_keeps_the_executable_bit` · `ariadna-git/src/conflict.rs` (12) · `core/merge.rs` (14) · `core/destructive.rs:escribir_resolucion_exige_confirmacion` · `ui/ConflictsPanel.test.ts` | CUBIERTO |
| G-14 marcar resuelto, continuar y abortar | M3 | `git/engine_branches.rs:abort_merge_restores_clean_tree`, `:rebase_conflict_abort_and_continue`, `:cherry_pick_conflict_abort`, `:cherry_pick_conflict_continue`, `:integration_state_follows_linked_worktree_git_dir` · `core/merge.rs:continuar_sin_mensaje_usa_el_de_git_y_resolver_exige_confirmacion`, `:cherry_pick_en_curso_se_detecta_continua_y_no_se_aborta_como_merge`, `:cherry_pick_se_puede_abortar_con_abort_integration` · `core/destructive.rs:abortar_integracion_con_y_sin_confirmacion` | CUBIERTO |
| G-15 recientes y favoritos + importación del `recents.json` | M1 | `ariadna-git/src/recents.rs` (14: `:reads_swift_written_file_and_writes_swift_compatible_json`, `:imports_swift_file_without_duplicating`, `:corrupt_main_recovers_from_backup`…) · `core/tabs.rs:favoritos_y_olvidar_recientes`, `:importa_el_recents_json_del_swift` | CUBIERTO |
| G-16 clonar (URL, credenciales, probar y olvidar) | M3/M4 | `git/clone.rs` (11: `:clon_feliz_con_rama_progreso_en_orden_y_repo_abierto`, `:urls_peligrosas_se_rechazan_sin_ejecutar_nada`, `:clona_un_repo_privado_con_pat_y_no_deja_rastro_del_token`, `:el_pat_en_clonados_locales_se_ignora_y_no_se_filtra`…) · `git/credentials.rs` (7) · `ariadna-git/src/askpass.rs` (4) · `core/clone_history.rs:clona_abre_una_pestana_y_emite_progreso`, `:un_401_es_auth_y_el_token_no_aparece_en_el_error_ni_en_disco`, `:credenciales_guardar_probar_y_olvidar_con_un_helper_falso` · `ui/CloneDialog.test.ts` | **PARCIAL**: falta el test de «cancelar clonado» que se ignora con git 2.53 (deuda anotada en `tasks.md` M4) y el humo con un proveedor real |

## 3. Pull requests (PR-01…PR-10)

Pruebas de `crates/ariadna-hosting/tests/github.rs` y `azure_devops.rs` corren contra un
servidor simulado (`wiremock`); las de GitHub se han ejecutado además contra la API real
(`github_real.rs`, 2026-11-11). **Azure DevOps no se ha probado nunca contra su API real.**

| ID | Tarea | Prueba (`archivo:test`) | Estado |
|---|---|---|---|
| PR-01 detectar proveedor/org/repo desde el remoto | M8 | `ariadna-hosting/src/remote.rs:tabla_de_urls`, `:nunca_deja_credenciales_en_el_resultado`, `:web_urls_de_azure_codifican_los_espacios`, `:urls_de_pull_request`, `:propiedad_nunca_entra_en_panico_con_entrada_aleatoria` · `core/pull_requests.rs:el_soporte_sale_del_remoto_y_no_usa_la_red`, `:gitlab_o_sin_remoto_no_tiene_prs_dentro_de_la_app`, `:se_prefiere_origin_y_si_no_el_primer_remoto_con_soporte` · `core/remotes.rs:lista_remotos_con_proveedor_y_capacidades`, `:enlaces_web_por_proveedor_con_host_del_remoto` | CUBIERTO |
| PR-02 listar (abiertas, mías, revisor, cerradas) | M8 | `github.rs:lista_envia_basic_cabeceras_de_github_y_sigue_link`, `:mias_y_revisor_filtran_por_el_usuario`, `:cerradas_distingue_fusionadas_de_cerradas`, `:resumen_es_una_sola_peticion` · `azure_devops.rs:lista_abiertas_envia_basic_y_api_version_y_pagina`, `:ultima_pagina_no_tiene_mas`, `:mias_y_revisor_usan_el_id_del_usuario_autenticado`, `:cerradas_pide_todo_y_descarta_las_activas` · `ui/PullRequestsPanel.test.ts` | CUBIERTO (simulado; GitHub real) |
| PR-03 detalle (descripción, revisores, políticas, work items, archivos) | M8 | `github.rs:detalle_calcula_votos_comprobaciones_commits_y_archivos`, `:detalle_de_pr_de_fork_lo_marca`, `:el_permiso_que_falta_viaja_en_el_detalle`, `:el_detalle_reintenta_mientras_mergeable_es_null` · `azure_devops.rs:detalle_junta_politicas_work_items_commits_y_archivos`, `:el_detalle_sobrevive_si_el_token_no_lee_politicas` · `ui/PrDetail.test.ts` (Markdown como texto, sin `{@html}`) | CUBIERTO (simulado; GitHub real) |
| PR-04 crear (plantilla, revisores, borrador, work items, «generar con IA») | M8 | `github.rs:crear_manda_head_base_borrador_y_pide_revisores`, `:crear_rechaza_revisores_raros_y_duplicadas_son_conflicto`, `:crear_con_revisor_rechazado_devuelve_la_pr_con_una_nota`, `:los_422_reales_se_clasifican_por_el_detalle_de_errors` · `azure_devops.rs:crear_envia_refs_borrador_revisores_y_work_items`, `:crear_valida_antes_de_llamar`, `:crear_duplicada_es_conflicto` · `core/pull_requests.rs:la_plantilla_se_busca_segun_el_proveedor`, `:crear_la_pr_publica_antes_la_rama_sin_upstream`, `:si_se_cancela_la_publicacion_no_se_crea_la_pr` · `core/ai.rs:propone_titulo_y_descripcion_de_pr_sin_tocar_el_repo`, `:pr_respeta_el_consentimiento_y_la_desactivacion`, `:pr_errores_de_base_y_ramas_sin_cambios`, `:pr_solo_con_archivos_sensibles_no_envia_nada` | CUBIERTO (simulado; GitHub real) |
| PR-05 bajar la rama de la PR (`refs/pull/N/head` incluido) | M8 | `git/pr_checkout.rs:baja_la_rama_de_una_pr_de_fork_y_se_cambia_a_ella`, `:la_segunda_vez_no_hace_nada_nuevo`, `:si_la_rama_local_existe_con_otra_base_no_se_toca_sin_permiso`, `:con_cambios_locales_que_chocan_avisa_antes_de_tocar_el_arbol`, `:una_referencia_que_el_remoto_no_tiene_es_error_tipado`, `:una_referencia_que_no_es_de_refs_se_rechaza_sin_red` · `core/pull_requests.rs:bajar_la_pr_de_un_fork_crea_la_rama_local_y_se_cambia_a_ella`, `:si_la_rama_local_ya_existe_con_otra_base_se_pide_confirmacion`, `:con_cambios_sin_confirmar_se_ofrecen_guardar_en_el_stash`, `:si_el_remoto_no_tiene_el_ref_lo_dice_sin_crear_rama` | CUBIERTO (implementado en paralelo a esta matriz; `pr_checkout.rs` y los tests de `core/pull_requests.rs` estaban **sin commit** al generarla, así que no los he ejecutado) |
| PR-06 revisar: diff de la PR, hilos en línea (ver, crear, responder, resolver) | M8 | `github.rs:archivos_paginan_con_link_y_rechazan_un_next_de_otro_host`, `:diff_compone_cabeceras_y_explica_los_parches_ausentes`, `:hilos_juntan_graphql_y_comentarios_generales`, `:crear_hilo_en_linea_general_responder_y_resolver` · `azure_devops.rs:hilos_filtran_sistema_borrados_y_mapean_estado_y_ancla`, `:crear_responder_y_resolver_hilos`, `:diff_genera_parches_unificados_desde_las_versiones` · `core/pull_requests.rs:listar_detalle_crear_hilos_y_diff_pasan_por_el_proveedor`, `:comment_input_se_lee_como_lo_manda_la_ui`, `:el_diff_del_proveedor_se_convierte_al_formato_del_visor` · `ui/PrFiles.test.ts` | CUBIERTO (simulado; GitHub real) |
| PR-07 votar (5 en Azure DevOps, 3 en GitHub) | M8 | `github.rs:votar_mapea_a_eventos_de_review_y_valida`, `:aprobar_la_propia_pr_es_policy` · `azure_devops.rs:votar_pone_el_voto_del_usuario_autenticado`, `:voto_solo_comentar_no_existe_en_azure_devops` · `core/pull_requests.rs:el_voto_de_rechazo_pide_confirmacion_y_el_resto_no`, `:un_voto_que_el_proveedor_no_tiene_se_rechaza_sin_preguntar` · `ui/PrDetail.test.ts:GitHub ofrece aprobar, solicitar cambios y comentar` | CUBIERTO (simulado; GitHub real) |
| PR-08 completar / abandonar / reabrir con confirmación y estrategias | M8 | `github.rs:completar_hace_squash_con_el_sha_y_borra_la_rama`, `:completar_respeta_405_409_y_estrategias_no_soportadas`, `:cerrar_y_reabrir` · `azure_devops.rs:completar_envia_estrategia_commit_de_origen_y_opciones`, `:auto_complete_no_manda_status_completed`, `:politica_incumplida_al_completar_es_policy`, `:abandonar_y_reabrir_cambian_el_estado` · `core/pull_requests.rs:abandonar_pide_confirmacion_que_nombra_repo_pr_y_destino`, `:en_github_abandonar_se_llama_cerrar`, `:completar_pide_confirmacion_con_estrategia_y_respeta_el_rechazo`, `:completar_valida_antes_de_preguntar` · `ui/PrDetail.test.ts:Azure DevOps: cuatro estrategias, auto-complete y elementos de trabajo` | CUBIERTO (simulado; GitHub real) |
| PR-09 refresco manual y opcional por intervalo (≥ 60 s, solo con el panel abierto) | M8 | — (el refresco **manual** existe en la UI; el intervalo **no está implementado**) | **HUECO** |
| PR-10 errores tipados (AUTH con permiso, NOT_FOUND, POLICY, RATE_LIMITED, NETWORK) | M8 | `github.rs:errores_graphql_se_tipan`, `:errores_http_se_tipan`, `:la_fabrica_solo_acepta_azure_devops_y_github`, `:las_capacidades_difieren_por_proveedor`, `:el_token_no_aparece_en_ningun_error_ni_debug` · `azure_devops.rs:errores_http_se_tipan`, `:token_invalido_redirige_o_responde_203_y_es_auth`, `:limite_de_uso_reintenta_con_retry_after_y_luego_funciona`, `:retry_after_demasiado_largo_no_se_espera`, `:un_503_se_reintenta_en_lecturas_pero_no_en_escrituras`, `:sin_servidor_es_network`, `:respuesta_mal_formada_o_gigante_es_parse` · `core/pull_requests.rs:los_errores_del_proveedor_llegan_tipados_y_sin_el_token` · `core/errors.rs:cada_git_error_tiene_su_codigo_exacto_y_mensaje_en_espanol`, `:los_codigos_cubren_exactamente_el_contrato` | CUBIERTO |
| §4.4 seguridad de red (origen único, redirecciones, tope, timeout) | M8 | `github.rs:archivos_paginan_con_link_y_rechazan_un_next_de_otro_host`, `:respuesta_mal_formada_o_gigante_es_parse` · `azure_devops.rs:sin_servidor_es_network`, `:respuesta_mal_formada_o_gigante_es_parse` · `ariadna-hosting/src/http.rs:solo_https_salvo_loopback_de_pruebas` · `ariadna-hosting/src/net.rs:*` (proxy/CA, ver §7) | CUBIERTO |

## 4. IA (RF-IA1…RF-IA12)

Pruebas con **CLI falso** (`crates/ariadna-ai/src/bin/fake_cli.rs` y `tests/cli.rs`) y con
un `git` falso; nada de esto se ha ejecutado con Codex ni Copilot reales.

| ID | Tarea | Prueba (`archivo:test`) | Estado |
|---|---|---|---|
| RF-IA1 prompt por stdin, nunca argv | M6 | `ai/cli.rs:el_prompt_va_por_stdin_y_no_por_argv`, `:las_opciones_de_commit_llegan_al_hijo_por_stdin`, `:explicar_commit_y_secretos_redactados_en_stdin`, `:pull_request_propone_titulo_y_descripcion_por_stdin_y_redactada` · `ariadna-ai/src/commands.rs:datos_del_repo_nunca_acaban_en_argv`, `:el_prompt_viaja_por_stdin`, `:el_resumen_no_incluye_el_prompt` · `ai/resolve.rs:lista_blanca_de_cmd_rechaza_datos_del_repo` | CUBIERTO |
| RF-IA2 resolución de binarios y shims `.cmd` | M6 | `ariadna-ai/src/resolve.rs:parsea_shim_cmd_de_npm`, `:parsea_shim_cmd_antiguo_con_crlf`, `:parsea_shim_ps1`, `:shim_irreconocible_devuelve_none`, `:cmd_shim_falla_si_recibe_un_argumento_fuera_de_la_lista`, `:node_entry_antepone_el_script`, `:prefiere_el_nativo_en_windows`, `:busca_en_scoop_y_volta` · `commands.rs:todo_argumento_de_codex_y_copilot_esta_en_la_lista_blanca_de_cmd` | CUBIERTO (en Linux; el shim `.cmd` real de Windows no se ha ejecutado) |
| RF-IA3 entorno por lista blanca y variables prohibidas | M6 | `ariadna-ai/src/env.rs:solo_pasa_la_lista_blanca`, `:variables_prohibidas_nunca_llegan_ni_siendo_allowlisted`, `:la_lista_blanca_es_exactamente_la_del_spec`, `:directorio_de_config_aislado_por_proveedor`, `:el_directorio_aislado_pisa_el_del_padre` · `ai/cli.rs:el_secreto_del_padre_y_las_claves_prohibidas_no_llegan_al_hijo`, `:copilot_recibe_el_token_solo_si_el_usuario_lo_declara` | CUBIERTO |
| RF-IA4 disponibilidad/versión/sesión ejecutando el CLI | M6 | `ai/cli.rs:disponibilidad_ejecuta_version`, `:la_cuenta_de_claude_sale_del_cli_y_lleva_el_directorio_aislado`, `:codex_y_copilot_no_exponen_cuenta_y_no_se_lanza_nada`, `:cuenta_con_cli_ausente_es_no_disponible` · `ariadna-ai/src/parse.rs:auth_status_de_claude_con_sesion`, `:auth_status_de_claude_sin_sesion_y_formatos_raros` · `core/settings.rs:cli_status_no_encontrado_sin_cli_y_con_comando_de_login` | CUBIERTO para Claude; **PARCIAL** para Codex/Copilot (sin CLI real) |
| RF-IA5 directorio de trabajo vacío y limpiado | M6 | `ai/cli.rs:el_directorio_de_trabajo_esta_vacio_y_se_limpia` | CUBIERTO |
| RF-IA6 ejecución: timeout, cancelación, tope de salida, sin TTY | M6 | `ai/cli.rs:timeout_duro`, `:cancelacion_mata_al_hijo`, `:tope_de_salida`, `:salida_no_reconocida_nunca_es_texto_crudo`, `:el_error_de_login_es_accionable` · `core/ai.rs:un_cli_colgado_acaba_en_timeout`, `:cancelar_mata_el_proceso_y_devuelve_cancelled` | CUBIERTO |
| RF-IA7 esquema de parseo por proveedor, goldens | M6 | `ariadna-ai/src/parse.rs` (26: `:claude_golden_con_banner_y_campos_extra`, `:claude_json_en_varias_lineas`, `:claude_sin_result_o_basura_es_formato_no_reconocido`, `:codex_golden_extrae_el_ultimo_mensaje_y_tolera_desconocidos`, `:codex_formato_antiguo_assistant_message`, `:codex_fallos`, `:copilot_texto_plano_y_ansi`, `:copilot_error_con_codigo_cero_se_clasifica`, `:mensaje_limpio_asunto_y_cuerpo`, `:propuesta_de_conflicto`…) · `ai/cli.rs:copilot_mensaje_de_commit_sin_git_ni_escritura`, `:codex_propone_un_conflicto_sin_aplicarlo` | **PARCIAL**: los goldens de Claude son reales; los de Codex/Copilot son reconstruidos de la documentación |
| RF-IA8 cola FIFO global, 2 simultáneas, límite por minuto | M6 | `ariadna-ai/src/orchestrator.rs:maximo_dos_llamadas_simultaneas`, `:limite_por_minuto`, `:cancelar_mientras_espera_turno` | CUBIERTO |
| RF-IA9 Auto salta solo si no está instalado | M6 | `orchestrator.rs:auto_order_solo_entre_habilitados_y_con_consentimiento`, `:auto_salta_si_no_esta_instalado_y_la_eleccion_explicita_no`, `:nunca_hay_fallback_por_sesion_cuota_o_contenido`, `:ninguno_instalado_en_auto` · `ai/cli.rs:orquestador_de_extremo_a_extremo_con_el_cli_falso` | CUBIERTO |
| RF-IA10 términos de uso citados con fecha | M0 | — (documento de cumplimiento: [`terminos-ia.md`](terminos-ia.md)) | CUBIERTO por documento, no por prueba (no es verificable por test) |
| RF-IA11 `policy.json` administrable (posterior a v0.1) | — | — | **HUECO por diseño** (fuera del alcance de v0.1, `tasks.md` «Después de v0.1») |
| RF-IA12 cuenta/organización visible | M6 | `ai/cli.rs:la_cuenta_de_claude_sale_del_cli_y_lleva_el_directorio_aislado`, `:dos_perfiles_ven_cada_uno_su_directorio_y_su_sesion`, `:codex_y_copilot_no_exponen_cuenta_y_no_se_lanza_nada` · `core/ai.rs:la_cuenta_sale_del_cli_con_el_directorio_de_cada_perfil` · `ui/ProposalSheet.test.ts:sin cuenta que mostrar dice «no disponible» en vez de inventarla` | CUBIERTO para Claude; «no disponible» explícito en los otros |

### Perfiles Personal/Empresa (D16) y consentimiento (§5.1)

| Requisito | Prueba | Estado |
|---|---|---|
| Perfil decide proveedores y consentimiento | `ariadna-ai/src/orchestrator.rs:valores_por_defecto_de_los_perfiles`, `:consentimiento_y_habilitacion`, `:interruptor_global`, `:perfil_serializa_ida_y_vuelta` · `core/ai.rs:la_regla_de_perfil_decide_con_que_perfil_se_genera_y_donde_se_guarda_el_consentimiento`, `:el_consentimiento_se_pide_una_vez_y_si_se_rechaza_no_se_envia_nada`, `:ia_desactivada_no_ejecuta_nada` · `ui/SettingsDialog.test.ts:activar/desactivar, perfil y proveedor se aplican al momento; Empresa deja Claude desactivado` | CUBIERTO |
| Aislamiento de login por perfil | `ariadna-ai/src/login.rs:claude_aislado_en_posix_y_cmd`, `:codex_usa_codex_home_y_copilot_nunca_lleva_variable`, `:el_script_limpia_variables_prohibidas_y_fija_el_aislamiento` · `env.rs:aislamiento_verificado_solo_en_claude` · `core/ai.rs:empresa_aisla_la_sesion_y_personal_usa_la_normal_hasta_que_se_active`, `:el_estado_avisa_de_los_proveedores_que_comparten_login`, `:copilot_comparte_login_y_el_login_avisa` | CUBIERTO (avisa de Copilot, que comparte login) |
| Vista previa, exclusión de sensibles, redacción de secretos, dato no fiable | `ariadna-ai/src/prompt.rs:rutas_sensibles`, `:el_diff_excluye_ficheros_sensibles`, `:redacta_cada_tipo_de_secreto`, `:redactar_es_idempotente`, `:el_prompt_delimita_el_dato_y_lleva_instruccion_de_sistema`, `:un_dato_malicioso_no_puede_cerrar_el_bloque`, `:la_vista_previa_refleja_exclusiones_redacciones_y_bytes`, `:rechaza_texto_fuera_de_los_hunks` · `core/ai.rs:los_archivos_sensibles_nunca_salen_del_equipo`, `:la_vista_previa_no_ejecuta_el_cli_ni_pregunta`, `:el_diff_enorme_se_rechaza_sin_llamar_al_cli` | CUBIERTO |

## 5. No funcionales (NF-1…NF-8)

| ID | Tarea | Prueba | Estado |
|---|---|---|---|
| NF-1 arranque ≤ 1,5 s | M7 | — (requiere ventana real; no medido) | **HUECO** |
| NF-2 primer `status` ≤ 300 ms | M7 | — (requiere ventana real) | **HUECO** |
| NF-3 `status` de 50 000 archivos ≤ 2 s | M2 | `git/perf.rs:nf3_status_untracked_50k`, `:nf3_status_modified_50k_and_stage_via_stdin` · `git/bulk_paths.rs` · informe `rendimiento.md` | CUBIERTO (medido, `#[ignore]` en CI) |
| NF-4 100 000 commits paginados, scroll 60 fps | M2/M5 | `git/engine_log_cursor.rs:cursor_pages_equal_skip_pages_with_merges_and_ties`, `:a_bad_cursor_is_rejected_before_reaching_git`, `:empty_repo_and_exact_size_page_have_no_next_page` · `git/perf.rs:nf4_log_ten_thousand_commits_paginated` · `ui/HistoryPanel.test.ts:con un historial enorme solo se pintan unas pocas filas (virtualizado) y se pagina` | CUBIERTO (motor y virtualización; 60 fps no medido) |
| NF-5 diff grande colapsado/binarios | M5 | `git/perf.rs:nf5_large_diffs` · `ariadna-git/src/diff.rs:binary_path_with_spaces_does_not_swallow_other_side`, `:truncated_and_garbage_input_never_panics` · `ui/DiffView.test.ts` | CUBIERTO |
| NF-6 20 pestañas sin agotar recursos | M4 | `core/tabs.rs:tope_blando_de_20_pestanas_avisa_sin_fallar`, `:cerrar_la_activa_pasa_a_la_vecina_y_actualiza_la_sesion` · hibernación en UI (sin prueba de memoria) | **PARCIAL**: falta medir memoria con 20 pestañas |
| NF-7 cancelación ≤ 5 s (árbol de procesos) | M1 | `process/runner.rs:timeout_termina_el_proceso`, `:cancelacion_termina_el_proceso`, `:timeout_mata_tambien_al_nieto` · `git/session.rs:close_durante_una_mutacion_larga_cancela_y_vacia_la_cola`, `:close_fuerza_la_cancelacion_si_la_operacion_no_coopera` | CUBIERTO |
| NF-8 sin telemetría; única red = API del proveedor | M8 | Prueba implícita: `core/pull_requests.rs:el_soporte_sale_del_remoto_y_no_usa_la_red` · `http.rs:solo_https_salvo_loopback_de_pruebas` · `net.rs:*` | **PARCIAL**: no hay prueba que afirme «no hay otra salida de red» |

## 6. Contrato IPC, confirmaciones y pestañas (§3.3, §6, §8)

| Requisito | Prueba | Estado |
|---|---|---|
| §3.3 errores tipados con código estable y mensaje en español | `core/errors.rs:cada_git_error_tiene_su_codigo_exacto_y_mensaje_en_espanol`, `:los_codigos_cubren_exactamente_el_contrato`, `:la_forma_json_es_code_message_detail`, `:el_detalle_lleva_el_dato_util_y_nunca_va_vacio`, `:errores_propios_de_la_app` | CUBIERTO |
| §3.3/§4.1 `DubiousOwnership` → confirmación y `safe.directory` solo de esa carpeta | `core/safe_directory.rs:sin_confirmacion_no_se_escribe_nada`, `:al_aceptar_se_anade_solo_esa_carpeta_y_nunca_asterisco`, `:una_ruta_que_no_vale_falla_sin_reintentar`, `:el_aviso_del_error_nombra_la_carpeta` | CUBIERTO (implementado en paralelo; fichero **sin commit** al generar esta matriz) |
| §3.3 `operation_id` y `cancel_operation` | `git/session.rs:los_ids_de_operacion_son_unicos_y_se_pueden_mostrar`, `:cancel_operation_saca_una_mutacion_de_la_cola_sin_ejecutarla`, `:cancel_operation_suelta_una_mutacion_en_curso_y_la_cola_sigue` · `core/tabs.rs:cancelar_una_operacion_por_su_id` · `core/ai.rs:emite_eventos_de_progreso_de_la_operacion` | CUBIERTO |
| §3.3 formas serializadas = contrato con la UI | `ui/api/contract.test.ts`, `ui/api/pr.contract.test.ts`, `ui/api/settings.contract.test.ts`, `ui/api/aiAlpha2.test.ts` · `core/pull_requests.rs:las_formas_serializadas_coinciden_con_los_fixtures_de_la_ui` · `core/settings.rs:formas_json_del_contrato_con_la_ui` | CUBIERTO |
| §6 confirmación nativa desde Rust con token de un solo uso | `core/destructive.rs` (11: `:discard_sin_confirmacion_no_toca_nada`, `:escribir_resolucion_exige_confirmacion`…) · `core/pull_requests.rs:abandonar_pide_confirmacion_que_nombra_repo_pr_y_destino`, `:completar_pide_confirmacion_con_estrategia_y_respeta_el_rechazo`, `:el_voto_de_rechazo_pide_confirmacion_y_el_resto_no` · `ui/*.test.ts` (varios «pide confirmación nativa») | CUBIERTO |
| §6 entorno de git saneado y repo hostil | `ariadna-process/src/env.rs:git_default_es_lista_blanca_y_quita_secretos`, `:allowlist_ignora_mayusculas_y_prefijos` · `git/engine_core.rs:reads_never_run_fsmonitor_external_diff_or_textconv`, `:hostile_pager_editor_and_credential_config_are_harmless`, `:unsafe_paths_are_rejected_before_git_runs` · `ariadna-git/src/path_safety.rs` (13) · `ariadna-git/src/auth.rs:solo_con_codigo_128` | CUBIERTO |
| §6 enlaces externos solo https y host del remoto | `core/external.rs:acepta_los_hosts_de_la_lista`, `:rechaza_http_y_esquemas_raros`, `:rechaza_otros_hosts_y_trucos`, `:enlaces_de_remoto_solo_https_y_su_host`, `:el_error_no_se_pasa_de_largo_con_urls_enormes` · `core/remotes.rs:enlaces_web_rechazan_lo_que_no_se_puede_validar` · `core/tabs.rs:pick_folder_pasa_por_el_prompter` | CUBIERTO |
| §8 pestañas: dedupe, reorden, cierre, tope | `core/tabs.rs` (18: `:dedupe_por_ruta_equivalente_con_enlace_simbolico`, `:reabrir_activa_la_pestana_existente`, `:reordena_ignorando_ids_desconocidos_y_conservando_los_que_faltan`, `:salir_con_trabajo_pendiente_sigue_el_mismo_camino`, `:tope_blando_de_20_pestanas_avisa_sin_fallar`…) · `git/session.rs:repo_key_worktrees_comparten_clave_con_el_mismo_common_dir`, `:repo_key_con_git_real_unifica_worktrees` · `ui/TabStrip.test.ts` | CUBIERTO |
| §8 vigilante de archivos (debounce, overflow → sondeo) | `core/watch.rs` + `core/watch/tests.rs` (18) | CUBIERTO |
| §7 accesibilidad (roles, foco, axe) | `ui/a11y.test.ts` (18 pantallas) · `ui/TabStrip.test.ts:expone roles tablist/tab con aria-selected` · `ui/SettingsDialog.test.ts:las flechas, Inicio y Fin cambian de sección y mueven el foco` | **PARCIAL**: axe en jsdom (sin contraste), sin NVDA/VoiceOver ni axe en E2E |
| §10 CI multi-plataforma | `.github/workflows/{rust,ui,bundle}.yml` | CUBIERTO por configuración (resultado no revisado en este entorno) |

## 7. Red: proxy y CA del sistema (M7, «mejor esfuerzo»)

Implementado en `crates/ariadna-hosting/src/net.rs` (`build_http_client()`), con pruebas
propias:

| Qué | Prueba (`ariadna-hosting/src/net.rs:test`) |
|---|---|
| Enrutado por esquema con `ALL_PROXY` de respaldo | `enruta_por_esquema_con_all_proxy_de_respaldo` |
| Proxy sin esquema → `http://` | `un_proxy_sin_esquema_se_interpreta_como_http` |
| Proxy mal escrito → error que nombra la variable y no repite el valor | `un_proxy_mal_escrito_es_un_error_que_no_repite_el_valor` |
| El proxy local recibe `CONNECT host:443` | `el_proxy_local_recibe_connect_con_el_host_del_proveedor` |
| `NO_PROXY=*` deja las peticiones directas | `no_proxy_salta_el_proxy_para_el_host_excluido` |
| `SSL_CERT_FILE` añade la CA de prueba y el handshake TLS verifica | `ssl_cert_file_anade_la_ca_de_prueba_y_el_tls_verifica` |
| `SSL_CERT_FILE` inexistente o sin PEM → error claro | `ssl_cert_file_inexistente_o_vacio_da_error_claro` |
| Cliente sin configuración | `un_cliente_sin_configuracion_se_construye` |

**Pendiente (honesto):** `http.rs` sigue construyendo su propio cliente con su política de
redirecciones; `apply_system_networking` (exportada por el crate) es la llamada de una línea
que ese constructor puede adoptar antes de `build()` para usar proxy y CA del sistema en
producción. Mientras no se conecte, las pull requests **no** usan todavía `net.rs` (el
`system-proxy` de `reqwest` cubre solo el proxy del sistema, no `SSL_CERT_FILE`). Sin verificar
en un equipo real detrás de proxy corporativo con inspección TLS.

## 8. Huecos, en una lista

1. **PR-09** refresco por intervalo (≥ 60 s): no implementado.
2. **RF-IA11** `policy.json` administrable: posterior a v0.1 por decisión (D16/RF-IA11).
3. **NF-1/NF-2** arranque y primer `status`: requieren ventana real.
4. **NF-6** memoria con 20 pestañas: sin medir.
5. **NF-8** «sin telemetría»: no hay prueba que lo afirme; es auditoría manual.
6. **Cancelación de clonado** con git 2.53 (deuda de `tasks.md` M4).
7. **Humo real** de Azure DevOps, Codex y Copilot: siguen pendientes.
8. **`net.rs` conectado a `http.rs`**: pendiente (ver §7).
9. **Verificar** los tests que otro agente escribía a la vez que esta matriz
   (`pr_checkout.rs`, `safe_directory.rs`): existen y están citados, pero no se han
   ejecutado en esta pasada.
