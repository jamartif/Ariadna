# Gates de CI: no se saltan

Regla de trabajo del repositorio. *(Este texto está pendiente de entrar en `AGENTS.md`, que es un
fichero protegido: se añade cuando Javier lo autorice. Mientras tanto, manda igual.)*

- **Prohibido mergear con `--admin`** (ni `gh pr merge --admin`, ni saltarse a mano un check
  obligatorio) para esquivar un gate. Si la CI está roja o incompleta: se investiga, se relanza el
  job o se espera el resultado. Solo se salta si **Javier lo autoriza expresamente** para ese caso, y
  entonces se dice en el resumen qué check quedó sin pasar y por qué.
- Un job que aparece **`cancelled`/`abandoned`** no es un test roto: míralo en el log o en las
  anotaciones del check-run. Si la anotación es «The job was not acquired by Runner…», es falta de
  runner: se relanza (o se deja que lo haga el workflow «Runner retry») y se espera el resultado real.
- El gate `rust-ok` clasifica los resultados con `scripts/ci/rust-gate.sh` (ok / fallo real / sin
  runner). Sus pruebas, `scripts/ci/rust-gate.test.sh`, se ejecutan en el CI (`gate del CI
  (pruebas)`): si el gate se rompe, se ve ahí y no al publicar una release.

## Por qué `cancelled` no es `failure`

Cuando GitHub Actions no consigue un runner (por ejemplo, capacidad de macOS arm64), **no** marca el
job como `failure`: lo deja en `cancelled` y anota en el check-run «The job was not acquired by
Runner of type hosted even after multiple attempts». Un agregador que exija `needs.*.result ==
success` convierte eso en rojo y tumba el pipeline sin un solo test roto — pasó en las alphas 8, 10 y
11, y la única salida era relanzar a mano.

Por eso `rust-ok` distingue tres estados:

| Estado | Cuándo | Código de `rust-gate.sh` | Qué hace el CI |
|---|---|---|---|
| **ok** | `success` | 0 | verde |
| **fallo real** | `failure`, o `cancelled` que **no** sea por falta de runner | 1 | rojo, con `::error::` por job. Nunca se relanza |
| **sin runner** | `cancelled`/`skipped` con la anotación «not acquired by Runner» | 3 (quedan intentos) / 0 (agotados) | con intentos pendientes: rojo a propósito y el workflow «Runner retry» relanza los jobs; agotados: verde con `::warning::` «sin runner tras 3 intentos» |

Así un fallo de verdad nunca se enmascara, y una falta de runner deja de confundirse con un test roto:
queda escrita como «sin runner tras N intentos», distinta de «test roto».
