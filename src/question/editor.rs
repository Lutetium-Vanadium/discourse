use std::{
    env,
    ffi::OsString,
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    process::Command,
};

use tempfile::TempPath;

use ui::{
    backend::{Backend, Stylize},
    error, widgets, Validation, Widget,
};

use super::{Filter, Options, Transform, Validate};
use crate::{Answer, Answers};

#[derive(Debug)]
pub struct Editor<'a> {
    extension: Option<String>,
    default: Option<String>,
    editor: OsString,
    filter: Filter<'a, String>,
    validate: Validate<'a, str>,
    transform: Transform<'a, str>,
}

impl<'a> Default for Editor<'a> {
    fn default() -> Self {
        Self {
            editor: get_editor(),
            extension: None,
            default: None,
            filter: Filter::None,
            validate: Validate::None,
            transform: Transform::None,
        }
    }
}

fn get_editor() -> OsString {
    env::var_os("VISUAL")
        .or_else(|| env::var_os("EDITOR"))
        .unwrap_or_else(|| {
            if cfg!(windows) {
                "notepad".into()
            } else {
                "vim".into()
            }
        })
}

struct EditorPrompt<'a, 'e> {
    prompt: widgets::Prompt<&'a str>,
    file: File,
    path: TempPath,
    ans: String,
    editor: Editor<'e>,
    answers: &'a Answers,
}

impl Widget for EditorPrompt<'_, '_> {
    fn render<B: Backend>(
        &mut self,
        layout: &mut ui::Layout,
        backend: &mut B,
    ) -> error::Result<()> {
        self.prompt.render(layout, backend)
    }

    fn height(&mut self, layout: ui::Layout) -> u16 {
        self.prompt.height(layout)
    }

    fn cursor_pos(&mut self, layout: ui::Layout) -> (u16, u16) {
        let (x, y) = self.prompt.cursor_pos(layout);
        if x == 0 {
            (layout.width - 1, y - 1)
        } else {
            (x - 1, y)
        }
    }

    fn handle_key(&mut self, _: ui::events::KeyEvent) -> bool {
        false
    }
}

impl ui::Prompt for EditorPrompt<'_, '_> {
    type ValidateErr = widgets::Text<String>;
    type Output = String;

    fn validate(&mut self) -> Result<Validation, Self::ValidateErr> {
        if !Command::new(&self.editor.editor)
            .arg(&self.path)
            .status()?
            .success()
        {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Could not open editor",
            )
            .into());
        }

        self.ans.clear();
        self.file.read_to_string(&mut self.ans)?;
        self.file.seek(SeekFrom::Start(0))?;

        if let Validate::Sync(ref mut validate) = self.editor.validate {
            validate(&self.ans, self.answers)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
        }

        Ok(Validation::Finish)
    }

    fn finish(self) -> Self::Output {
        match self.editor.filter {
            Filter::Sync(filter) => filter(self.ans, self.answers),
            _ => self.ans,
        }
    }

    fn has_default(&self) -> bool {
        false
    }
}

impl Editor<'_> {
    pub(crate) fn ask<B: Backend>(
        mut self,
        message: String,
        answers: &Answers,
        b: &mut B,
        events: &mut ui::events::Events,
    ) -> error::Result<Answer> {
        let mut builder = tempfile::Builder::new();

        if let Some(ref extension) = self.extension {
            builder.suffix(extension);
        }

        let mut file = builder.tempfile()?;

        if let Some(ref default) = self.default {
            file.write_all(default.as_bytes())?;
            file.seek(SeekFrom::Start(0))?;
            file.flush()?;
        }

        let transform = self.transform.take();

        let (file, path) = file.into_parts();

        let ans = ui::Input::new(
            EditorPrompt {
                prompt: widgets::Prompt::new(&*message)
                    .with_hint("Press <enter> to launch your preferred editor.")
                    .with_delim(widgets::Delimiter::None),
                editor: self,
                file,
                path,
                ans: String::new(),
                answers,
            },
            b,
        )
        .run(events)?;

        match transform {
            Transform::Sync(transform) => transform(&ans, answers, b)?,
            _ => {
                widgets::Prompt::write_finished_message(&message, b)?;
                b.write_styled(&"Received".dark_grey())?;
                b.write_all(b"\n")?;
                b.flush()?;
            }
        }

        Ok(Answer::String(ans))
    }
}

pub struct EditorBuilder<'a> {
    opts: Options<'a>,
    editor: Editor<'a>,
}

impl<'a> EditorBuilder<'a> {
    pub(crate) fn new(name: String) -> Self {
        EditorBuilder {
            opts: Options::new(name),
            editor: Default::default(),
        }
    }

    pub fn default<I: Into<String>>(mut self, default: I) -> Self {
        self.editor.default = Some(default.into());
        self
    }

    pub fn extension<I: Into<String>>(mut self, extension: I) -> Self {
        self.editor.extension = Some(extension.into());
        self
    }

    crate::impl_options_builder!();
    crate::impl_filter_builder!(String; editor);
    crate::impl_validate_builder!(str; editor);
    crate::impl_transform_builder!(str; editor);

    pub fn build(self) -> super::Question<'a> {
        super::Question::new(self.opts, super::QuestionKind::Editor(self.editor))
    }
}

impl<'a> From<EditorBuilder<'a>> for super::Question<'a> {
    fn from(builder: EditorBuilder<'a>) -> Self {
        builder.build()
    }
}
