//! Etiquetas de una pestaña: listar, crear (ligera o anotada), borrar en local, publicar y
//! borrar en el remoto. Los borrados piden confirmación nativa (`Prompter`) y solo después se
//! construye la `Confirmation` del motor.

use ariadna_git::{Confirmation, GitError, Tag};

use super::api::Done;
use super::app::AppCore;
use super::error::ApiResult;

impl AppCore {
    /// Etiquetas del repositorio, de la más reciente a la más antigua.
    pub async fn list_tags(&self, tab_id: &str) -> ApiResult<Vec<Tag>> {
        self.read_op(tab_id, "etiquetas", |e, r| {
            Box::pin(async move { e.tags(&r).await })
        })
        .await
    }

    /// Crea una etiqueta en `target` (HEAD si es `None`). Con `message` no vacío es anotada.
    pub async fn create_tag(
        &self,
        tab_id: &str,
        name: String,
        target: Option<String>,
        message: Option<String>,
    ) -> ApiResult<Tag> {
        self.mutate_op(
            tab_id,
            "crear etiqueta",
            (name, target, message),
            |e, r, (n, t, m)| {
                Box::pin(
                    async move { e.create_tag(&r, n.trim(), t.as_deref(), m.as_deref()).await },
                )
            },
        )
        .await
    }

    /// Borra una etiqueta local tras confirmación nativa que nombra repo, etiqueta y destino.
    pub async fn delete_tag(&self, tab_id: &str, name: String) -> ApiResult<Done> {
        let (s, meta) = self.live(tab_id)?;
        // Sin confirmación no se borra nada: si ya no existe, se dice y se acaba.
        let tags = self.list_tags(tab_id).await?;
        let Some(tag) = tags.iter().find(|t| t.name == name) else {
            return Err(GitError::TagNotFound { name }.into());
        };
        let kind = if tag.annotated {
            "anotada: se pierde su mensaje"
        } else {
            "ligera"
        };
        let ok = self
            .inner
            .prompter
            .confirm(
                "Borrar etiqueta",
                &format!(
                    "En {}: se borrará la etiqueta local «{name}» ({kind}). Los commits no se tocan.",
                    Self::context(&s, &meta)
                ),
                &format!(
                    "Etiqueta: {name}\nApunta a: {}\nRepositorio: {}",
                    tag.commit, meta.path
                ),
            )
            .await;
        if !ok {
            return Ok(Done { done: false });
        }
        let confirmation = Confirmation::granted("borrar la etiqueta");
        s.mutate("borrar etiqueta", move |e, r| {
            let (n, c) = (name.clone(), confirmation.clone());
            Box::pin(async move { e.delete_tag(&r, &n, &c).await })
        })
        .await?;
        Ok(Done { done: true })
    }

    /// Publica una etiqueta en el remoto (`git push <remote> refs/tags/<name>`).
    pub async fn push_tag(&self, tab_id: &str, remote: String, name: String) -> ApiResult<()> {
        self.mutate_op(
            tab_id,
            "publicar etiqueta",
            (remote, name),
            |e, r, (rm, n)| Box::pin(async move { e.push_tag(&r, &rm, &n).await }),
        )
        .await
    }

    /// Borra una etiqueta del **remoto** tras confirmación nativa (otros usuarios la pierden).
    /// La etiqueta local no se toca.
    pub async fn delete_remote_tag(
        &self,
        tab_id: &str,
        remote: String,
        name: String,
    ) -> ApiResult<Done> {
        let (s, meta) = self.live(tab_id)?;
        let ok = self
            .inner
            .prompter
            .confirm(
                "Borrar etiqueta del remoto",
                &format!(
                    "En {}: se borrará la etiqueta «{name}» del remoto «{remote}». Quien la haya descargado la conservará, pero dejará de existir en el servidor. La etiqueta local no se toca.",
                    Self::context(&s, &meta)
                ),
                &format!(
                    "Etiqueta: {name}\nRemoto: {remote}\nRepositorio: {}",
                    meta.path
                ),
            )
            .await;
        if !ok {
            return Ok(Done { done: false });
        }
        let confirmation = Confirmation::granted("borrar la etiqueta del remoto");
        s.mutate("borrar etiqueta del remoto", move |e, r| {
            let (rm, n, c) = (remote.clone(), name.clone(), confirmation.clone());
            Box::pin(async move { e.delete_remote_tag(&r, &rm, &n, &c).await })
        })
        .await?;
        Ok(Done { done: true })
    }
}
