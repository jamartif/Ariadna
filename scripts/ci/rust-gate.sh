#!/usr/bin/env bash
#
# Clasifica el resultado del workflow «Rust» en **tres** estados, no dos.
#
# Por qué existe: en GitHub Actions, si un runner no está disponible, el job **no falla**:
#   · GitHub lo cancela antes de empezar (`conclusion: cancelled`) y anota en el check-run
#     «The job was not acquired by Runner of type hosted even after multiple attempts».
#   · El agregador antiguo miraba `needs.test.result` y veía «cancelled», que trataba como rojo.
# Resultado: el pipeline entero aparecía fallido sin un solo test roto, y había que relanzar a mano
# (pasó en las alphas 8, 10 y 11).
#
# Los tres estados que distingue este script:
#   · ok          → el job terminó en success.
#   · fallo real  → failure, o cancelled **que no sea** por falta de runner (podría tapar algo).
#   · sin runner  → cancelled/skipped con la anotación «not acquired by Runner».
#
# Códigos de salida (los usa `.github/workflows/rust.yml` y `.github/workflows/runner-retry.yml`):
#   0 → todo bien, o «sin runner» cuando ya se agotaron los reintentos (aviso, no error)
#   1 → hay un fallo real (nunca se enmascara)
#   3 → solo falta runner y quedan reintentos: hay que relanzar el job
#
# Uso:
#   rust-gate.sh --jobs jobs.json [--no-runner ids.txt] [--attempt 1] [--max 3] [--summary file]
#
#   --jobs      JSON de `GET /repos/{owner}/{repo}/actions/runs/{id}/jobs` (con `.jobs[]`).
#   --no-runner archivo con los **ids** de los jobs cuyo check-run dice «not acquired by Runner».
#   --attempt   `github.run_attempt` del run (1 la primera vez).
#   --max       número máximo de intentos antes de dar el «sin runner» por definitivo (3).
#   --summary   si se indica, el resumen también se escribe en formato Markdown para el job summary.

set -uo pipefail

jobs_file=""
no_runner_file=""
attempt=1
max=3
summary_file=""

while [ $# -gt 0 ]; do
  case "$1" in
    --jobs) jobs_file="${2:-}"; shift 2 ;;
    --no-runner) no_runner_file="${2:-}"; shift 2 ;;
    --attempt) attempt="${2:-1}"; shift 2 ;;
    --max) max="${2:-3}"; shift 2 ;;
    --summary) summary_file="${2:-}"; shift 2 ;;
    *) echo "argumento desconocido: $1" >&2; exit 64 ;;
  esac
done

if [ -z "$jobs_file" ] || [ ! -f "$jobs_file" ]; then
  echo "falta --jobs con un JSON de jobs del run" >&2
  exit 64
fi
if ! command -v jq >/dev/null 2>&1; then
  echo "hace falta jq" >&2
  exit 64
fi
if ! [[ "$attempt" =~ ^[0-9]+$ ]] || ! [[ "$max" =~ ^[0-9]+$ ]]; then
  echo "--attempt y --max tienen que ser números" >&2
  exit 64
fi

# Ids de jobs que se quedaron sin runner (una anotación lo dice en el check-run).
is_no_runner() {
  [ -n "$no_runner_file" ] && [ -f "$no_runner_file" ] && grep -qx "$1" "$no_runner_file"
}

ok_list=()
fail_list=()
norunner_list=()

if ! rows=$(jq -er '.jobs[] | [(.id|tostring), .name, (.conclusion // "pending")] | @tsv' < "$jobs_file"); then
  # Un gate que no puede leer los resultados **no** puede dar verde.
  echo "no se pudo leer $jobs_file con jq: el gate no puede clasificar el resultado" >&2
  exit 64
fi
# Sin CR: en Windows `jq` termina las líneas con CRLF y la conclusión acabaría siendo "success\r".
rows=$(printf '%s\n' "$rows" | tr -d '\r')

while IFS=$'\t' read -r id name conclusion; do
  [ -n "$id" ] || continue
  # El agregador no se clasifica a sí mismo, y el job de ARM es «mejor esfuerzo» a propósito.
  case "$name" in
    rust-ok) continue ;;
    *"mejor esfuerzo"*) continue ;;
  esac
  case "$conclusion" in
    success)
      ok_list+=("$name")
      ;;
    cancelled|skipped)
      if is_no_runner "$id"; then
        norunner_list+=("$name")
      else
        # Cancelado por otra cosa (concurrencia, alguien lo paró…): puede tapar un fallo real.
        fail_list+=("$name ($conclusion)")
      fi
      ;;
    *)
      fail_list+=("$name ($conclusion)")
      ;;
  esac
done <<< "$rows"

# Si no se clasificó nada (JSON vacío o con nombres inesperados) tampoco se puede dar verde.
if [ ${#ok_list[@]} -eq 0 ] && [ ${#fail_list[@]} -eq 0 ] && [ ${#norunner_list[@]} -eq 0 ]; then
  echo "no se ha podido clasificar ningún job de $jobs_file" >&2
  exit 64
fi

echo "== Resultado del workflow Rust (intento ${attempt}/${max})"
if [ ${#ok_list[@]} -gt 0 ]; then
  echo "   ok:        ${ok_list[*]}"
else
  echo "   ok:        —"
fi
if [ ${#norunner_list[@]} -gt 0 ]; then
  echo "   sin runner: ${norunner_list[*]}"
fi
if [ ${#fail_list[@]} -gt 0 ]; then
  echo "   fallo:     ${fail_list[*]}"
fi

markdown=""
if [ -n "$summary_file" ]; then
  {
    echo "### Rust · intento ${attempt} de ${max}"
    echo ""
    echo "| estado | jobs |"
    echo "|---|---|"
    echo "| ok | ${ok_list[*]:-—} |"
    [ ${#norunner_list[@]} -gt 0 ] && echo "| sin runner | ${norunner_list[*]} |"
    [ ${#fail_list[@]} -gt 0 ] && echo "| fallo | ${fail_list[*]} |"
  } > "$summary_file"
  markdown=$(cat "$summary_file")
fi

# 1) Un fallo real siempre manda: nunca se enmascara.
if [ ${#fail_list[@]} -gt 0 ]; then
  for job in "${fail_list[@]}"; do
    echo "::error title=Test roto::${job} falló de verdad (no es falta de runner)."
  done
  [ -n "$markdown" ] && echo "$markdown" >> "${GITHUB_STEP_SUMMARY:-/dev/null}"
  echo "→ fallo real: no se publica ni se relanza nada."
  exit 1
fi

# 2) Solo falta runner.
if [ ${#norunner_list[@]} -gt 0 ]; then
  if [ "$attempt" -lt "$max" ]; then
    for job in "${norunner_list[@]}"; do
      echo "::error title=Sin runner::${job} no llegó a ejecutarse (GitHub no le asignó runner, intento ${attempt} de ${max}); se relanzará automáticamente."
    done
    [ -n "$markdown" ] && echo "$markdown" >> "${GITHUB_STEP_SUMMARY:-/dev/null}"
    echo "→ sin runner y quedan intentos: relanzar."
    exit 3
  fi
  for job in "${norunner_list[@]}"; do
    echo "::warning title=Sin runner::${job}: sin runner tras ${max} intentos. No es un fallo de tests."
  done
  [ -n "$markdown" ] && echo "$markdown" >> "${GITHUB_STEP_SUMMARY:-/dev/null}"
  echo "→ sin runner tras ${max} intentos: se deja constancia y no se cuenta como test roto."
  exit 0
fi

# 3) Todo verde.
[ -n "$markdown" ] && echo "$markdown" >> "${GITHUB_STEP_SUMMARY:-/dev/null}"
echo "→ todo correcto."
exit 0
