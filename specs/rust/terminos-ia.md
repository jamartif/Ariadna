# Términos de uso de la IA: Claude, Codex y GitHub Copilot

Documento de cumplimiento para **RF-IA10** (spec §5.3) y para la tarea M0 «leer y citar con
fecha los términos de Anthropic, OpenAI y GitHub para uso en empresa».

- **Fecha de consulta: 2026-11-13.** Las páginas se leyeron con `curl` desde este equipo y
  se citan con su fecha de entrada en vigor o de última actualización tal como figura en
  ellas. Los términos cambian: **vuelve a comprobarlos antes de fiarte de este resumen**.
- No es asesoramiento legal. El objetivo es decir qué cláusula importa y qué tiene que revisar
  la empresa antes de mandar código corporativo a un CLI de IA.

## 0. Cómo encaja Ariadna (y qué no hace)

Ariadna **no actúa como una app de terceros con claves de API**: lanza el **CLI oficial** en
nombre del usuario, con la sesión que ese usuario ya tiene en el equipo, y el prompt va por
stdin. No lee ni guarda credenciales, y **no gestiona planes ni condiciones de cuenta**
(decisión D14 del spec: uso particular y en empresa, sin despliegue gestionado).

Consecuencia práctica: **el contrato que aplica es el de la cuenta con la que está logueado
el CLI**, no uno entre Ariadna y el proveedor. Por eso el perfil **Empresa** de Ariadna
existe (D16): para que no se mande código de la empresa con la cuenta personal.

## 1. Anthropic (Claude y Claude Code)

| Documento | Vigencia | A quién aplica |
|---|---|---|
| [Consumer Terms of Service](https://www.anthropic.com/legal/consumer-terms) | **Effective Oct 8, 2025** | Personas: Claude.ai, **Claude Pro**, Max, Ultra y apps asociadas |
| [Commercial Terms of Service](https://www.anthropic.com/legal/commercial-terms) | **Effective Jun 17, 2025** | Empresas (Claude Team y Enterprise) |
| [Use Claude Code with your Pro, Max, or Ultra plan](https://support.claude.com/en/articles/11145838) | Actualizado «over 2 weeks ago» (consultado 2026-11-13) | Consumidores con plan individual |
| [Use Claude Code with your Team or Enterprise plan](https://support.claude.com/en/articles/11845131) | ídem | Miembros de organizaciones Team/Enterprise |

**Lo que dice el Consumer Terms sobre datos y sobre uso profesional:**

- *«Our use of Materials. We may use Materials to provide, maintain, and improve the
  Services and to develop other products and services, including training our models,
  unless you opt out of training through your account settings.»* → En un plan
  **individual**, tus entradas (código incluido) pueden usarse para entrenar **salvo que
  desactives** la opción en ajustes de la cuenta. Aun optando por no entrenar, se usan si
  mandas *feedback* o si el contenido se marca para revisión de seguridad.
- *«Non-commercial use only. You agree that you will not use our Services for any commercial
  or business purposes…»* → El plan de consumidor está escrito para uso **no comercial**.
- *«Business Domains. If you use an email address owned by your employer…, your Account may
  be linked to the organization's enterprise account… and the organization's administrator
  may be able to monitor and control the Account.»* → Una cuenta creada con correo de la
  empresa puede acabar absorbida por la cuenta de empresa.

**Lo que dice el Commercial Terms:** *«Anthropic may not train models on Customer Content
from Services.»* Customer conserva la propiedad de sus entradas y de las salidas. Es la
diferencia clave frente al plan individual.

**Claude Code:** está incluido en los planes Pro/Max/Ultra (artículo citado) **y** en cada
asiento de Team y Enterprise (Premium en algunos planes antiguos). El artículo de consumo
dice literalmente que sirve para codificar «at work and at home», lo que **choca con la
cláusula de uso no comercial** del mismo documento: es una tensión real entre marketing y
contrato, y la empresa de Javier debe decidir con qué plan se trabaja (no hay nada aquí que
podamos resolver nosotros).

**Detalle operativo que sí está verificado:** si existe `ANTHROPIC_API_KEY`, Claude Code la
usa **en lugar de la suscripción** y factura por API. Ariadna elimina `ANTHROPIC_API_KEY`,
`ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `CLAUDE_CODE_USE_BEDROCK` y
`CLAUDE_CODE_USE_VERTEX` del entorno del hijo (RF-IA3), precisamente para que la sesión sea
siempre la de la suscripción. Prueba: `crates/ariadna-ai/src/env.rs:variables_prohibidas_nunca_llegan_ni_siendo_allowlisted`.

## 2. OpenAI (Codex / ChatGPT)

| Documento | Vigencia | A quién aplica |
|---|---|---|
| [OpenAI Services Agreement](https://openai.com/policies/business-terms/) | **Effective January 1, 2026** (actualizado el 1 de diciembre de 2025) | Empresas y desarrolladores: **API**, **ChatGPT Business**, **ChatGPT Enterprise**; «does not apply to OpenAI services used by consumers or individuals» |
| [Terms of Use (Europe)](https://openai.com/policies/terms-of-use/) | **Updated January 16, 2026** | Consumidores del EEE, Suiza y Reino Unido (incluye ChatGPT Plus) |
| [Codex Pricing](https://chatgpt.com/codex/pricing/) | Consultado 2026-11-13 | — |

**Codex y los planes:** la página de precios dice *«Codex is included in your ChatGPT Free,
Go, Plus, Pro, Business, or Enterprise plan»*. Es decir: **Codex CLI funciona con cualquier
plan de ChatGPT**, incluidos los individuales, sin suscripción aparte.

**Terms of Use de consumo (Europe):**

- *«These Terms of Use (“Terms”) apply to your use of ChatGPT, DALL·E, and OpenAI's other
  services for individuals… including personal, non-commercial use of our Services by
  consumers.»*
- *«Opt out. If you do not want us to use your Content to train our models, you have the
  option to opt out by updating your account settings.»*
- *«Corporate domains. If you create an account using an email address owned by an
  organisation (for example, your employer), that account may be added to the organisation's
  business account… the organisation's administrator may monitor and control your account.»*

**Services Agreement (empresas):** solo aplica a API, Business y Enterprise; no cubre el uso
de un empleado con su ChatGPT Plus. La página enlaza su propio PDF y las condiciones de
tratamiento de datos; no se ha verificado aquí el detalle de entrenamiento del articulado
(la página en sí se ha leído, el PDF no).

**Riesgo concreto para Javier:** usar **Codex CLI con un plan Plus personal** para código de
la empresa cae fuera del Services Agreement y dentro de unos términos de consumo que hablan
de uso personal y no comercial, con entrenamiento desactivable pero **activado por defecto**.

## 3. GitHub Copilot

| Documento | Vigencia | A quién aplica |
|---|---|---|
| [GitHub Terms of Service](https://docs.github.com/en/site-policy/github-terms/github-terms-of-service) | **Effective date: April 27, 2026** | Todos; §J es la sección de IA |
| [GitHub Terms for Additional Products and Features](https://docs.github.com/en/site-policy/github-terms/github-terms-for-additional-products-and-features) | **Effective Date: August 27, 2026** | Copilot y otros productos |
| GitHub Generative AI Services Terms (`https://github.com/customer-terms/github-generative-ai-services-terms`, enlazado desde la página anterior) | — | Copilot **Business/Enterprise comprados directamente a GitHub** |
| [About GitHub Copilot CLI](https://docs.github.com/en/copilot/concepts/agents/copilot-cli/about-copilot-cli) | Consultado 2026-11-13 | — |

**§J.1 del Terms of Service (quién entrena con tus datos):**

> *«The training and data-use provisions in Section J.3 apply only to individual licenses.
> If your use of the Service is governed by a GitHub Customer Agreement or volume licensing
> agreement, those agreements govern the use of your data in connection with AI Features and
> Section J.3 does not apply to you.»*

**§J.3 (licencias individuales):**

> *«You also grant GitHub and its Affiliates a license to collect and use your Inputs and
> Outputs to develop, train and improve artificial intelligence and machine learning models…
> unless (a) you opt out through your account settings, or (b) your use of the Service is
> governed by a GitHub Customer Agreement or volume licensing agreement.»*

Con dos matices que el propio texto añade: el opt-out **no es retroactivo** («from the
effective date of your opt-out going forward») y la licencia **no** permite compartir tus
entradas o salidas con proveedores externos de modelos «for their own independent model
training purposes».

**Qué contrato aplica a Copilot** (Terms for Additional Products):

> *«For GitHub Copilot Business and Copilot Enterprise license holders who purchase directly
> from GitHub, your use of GitHub Copilot is governed by the GitHub Generative AI Services
> Terms. If you purchase through Microsoft, your use of GitHub Copilot is governed by
> Microsoft's Product Terms. For all other GitHub Copilot users, your use of GitHub Copilot
> is governed by Section J (AI Features) of the GitHub Terms of Service.»*

Es decir: **Copilot Pro/Pro+ individual → §J del TOS (entrena salvo opt-out)**; **Copilot
Business/Enterprise → contrato de organización, sin entrenamiento**.

**Copilot CLI:** *«GitHub Copilot CLI is available with all Copilot plans. If you receive
Copilot from an organization, the Copilot CLI policy must be enabled in the organization's
settings.»* → En una empresa, el CLI puede no funcionar hasta que un administrador habilite
la política; el perfil **Empresa** de Ariadna no puede arreglarlo.

**No verificado:** la página de los *GitHub Generative AI Services Terms* devolvió **error
del servidor de GitHub** (HTTP 500/página de error) al consultarla el 2026-11-13, igual que
su variante en `docs.github.com` (404). La URL se cita tal como la enlaza GitHub, pero **su
contenido no se ha podido leer**, así que no afirmamos nada sobre el detalle de esa parte.

## 4. Individual frente a organización, en una tabla

| Proveedor | Plan individual | Plan de organización |
|---|---|---|
| **Anthropic** | Consumer Terms (Oct 8, 2025): uso **no comercial**, entrenamiento salvo opt-out | Commercial Terms (Jun 17, 2025): **no entrena** con Customer Content; Customer es dueño de las salidas |
| **OpenAI** | Terms of Use Europe (Jan 16, 2026): uso personal, **no comercial**; opt-out de entrenamiento en ajustes | Services Agreement (Jan 1, 2026): solo API/Business/Enterprise; condiciones de datos del contrato |
| **GitHub** | TOS §J.3 (abr 27, 2026): licencia para **entrenar** salvo opt-out, sin efecto retroactivo | Contrato de organización / Generative AI Services Terms (no verificado): §J.3 **no aplica** |

## 5. Qué debe revisar la empresa de Javier antes de usar IA con repos corporativos

1. **Qué cuenta usa cada desarrollador y a nombre de quién.** Si la cuenta es individual
   (Claude Pro, ChatGPT Plus, Copilot Pro) y el correo es corporativo, los términos de
   consumo aplican, y Anthropic y OpenAI avisan de que la cuenta **puede quedar vinculada y
   monitorizada por la organización**. Decidir si eso se acepta.
2. **¿Hay contrato de organización con el proveedor?** Si la empresa tiene Claude Team/
   Enterprise, ChatGPT Business/Enterprise o Copilot Business/Enterprise, la IA debe usarse
   **con esa cuenta** (perfil Empresa de Ariadna) y no con la personal. Si no lo hay, la
   vía correcta es contratarlo, no usar el plan personal con código de clientes.
3. **Uso comercial.** Anthropic (Consumer) y OpenAI (Europe ToU) limitan el plan individual a
   uso **no comercial**. Codificar para la empresa con ese plan es, como mínimo, una zona
   gris contractual. Confirmar por escrito con el proveedor o con su asesoría.
4. **Entrenamiento y opt-out.** En planes individuales, desactivar el uso de datos para
   entrenamiento **antes** de empezar (Anthropic: ajustes de cuenta; OpenAI: ajustes;
   GitHub: ajustes personales de Copilot). El opt-out no repara lo ya enviado.
5. **Política de Copilot CLI.** Si la empresa usa Copilot Business/Enterprise, un
   administrador debe habilitar la política de **Copilot CLI** o el CLI no funcionará en los
   equipos.
6. **Datos y confidencialidad.** Revisar el DPA/condiciones de tratamiento de datos del
   contrato de empresa: dónde se procesa el código, cuánto se retiene, si hay subencargados
   y si el contrato cubre **código de terceros** (clientes) que a veces vive en el repo.
7. **Reglas internas.** Ariadna no puede cumplir por ti las normas de la empresa sobre
   enviar código a terceros (NDA, propiedad intelectual, secretos). El consentimiento por
   perfil y proveedor, la vista previa y la exclusión de ficheros sensibles ayudan, pero la
   decisión es de la empresa.
8. **Evidencia.** Ariadna puede guardar localmente un registro de lo enviado (opcional,
   §5.1). Si la empresa necesita auditoría, conviene activarlo antes del primer uso.

## 6. Lo que Ariadna garantiza (y lo que no)

**Garantiza** (con pruebas en el repositorio): lanza el CLI oficial, prompt por stdin, entorno
por lista blanca, sin claves de API (las borra), consentimiento por perfil y proveedor,
 vista previa, exclusión de `.env*`/`*.pem`/`id_*`/`*.key`/`*.pfx`, redacción de secretos y
 que la IA **solo propone**.

**No garantiza**: que tu plan permita el uso que haces, que el proveedor no entrene con tus
datos (eso depende de tu plan y de tus ajustes), ni que el CLI oficial respete la
residencia de datos que tu empresa exige. Eso es contrato, no software.

## 7. Fuentes consultadas (2026-11-13)

- Anthropic: [Consumer Terms](https://www.anthropic.com/legal/consumer-terms) ·
  [Commercial Terms](https://www.anthropic.com/legal/commercial-terms) ·
  [Claude Code con Pro/Max/Ultra](https://support.claude.com/en/articles/11145838) ·
  [Claude Code con Team/Enterprise](https://support.claude.com/en/articles/11845131)
- OpenAI: [Services Agreement](https://openai.com/policies/business-terms/) ·
  [Terms of Use (Europe)](https://openai.com/policies/terms-of-use/) ·
  [Codex Pricing](https://chatgpt.com/codex/pricing/)
- GitHub: [Terms of Service](https://docs.github.com/en/site-policy/github-terms/github-terms-of-service) ·
  [Terms for Additional Products and Features](https://docs.github.com/en/site-policy/github-terms/github-terms-for-additional-products-and-features) ·
  [Copilot CLI](https://docs.github.com/en/copilot/concepts/agents/copilot-cli/about-copilot-cli)

No se pudo leer: GitHub Generative AI Services Terms (error del servidor),
`help.openai.com` (requiere JavaScript) y el PDF del Services Agreement.
