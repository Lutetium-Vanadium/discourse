use std::fmt;

use crossterm::{
    cursor, queue,
    style::{Color, Colorize, ResetColor, SetForegroundColor},
    terminal,
};
use fxhash::FxHashSet as HashSet;
use ui::{widgets, Validation, Widget};

use crate::{error, Answer, Answers, ExpandItem};

use super::{none, some, Choice, Options, Transformer};

pub struct Expand<'t> {
    choices: super::ChoiceList<ExpandItem>,
    selected: Option<char>,
    default: char,
    transformer: Option<Box<Transformer<'t, ExpandItem>>>,
}

impl fmt::Debug for Expand<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Expand")
            .field("default", &self.default)
            .field("selected", &self.selected)
            .field("choices", &self.choices)
            .field(
                "transformer",
                &self.transformer.as_ref().map_or_else(none, some),
            )
            .finish()
    }
}

impl Default for Expand<'static> {
    fn default() -> Self {
        Expand {
            default: 'h',
            selected: None,
            choices: Default::default(),
            transformer: None,
        }
    }
}

struct ExpandPrompt<'t, F> {
    message: String,
    hint: String,
    list: widgets::ListPicker<Expand<'t>>,
    input: widgets::CharInput<F>,
    expanded: bool,
}

impl<F> ExpandPrompt<'_, F> {
    fn finish_with(self, c: char) -> ExpandItem {
        self.list
            .finish()
            .choices
            .choices
            .into_iter()
            .filter_map(|choice| match choice {
                Choice::Choice(choice) => Some(choice),
                _ => None,
            })
            .find(|item| item.key == c)
            .unwrap()
    }
}

impl<F: Fn(char) -> Option<char>> ui::Prompt for ExpandPrompt<'_, F> {
    type ValidateErr = &'static str;
    type Output = ExpandItem;

    fn prompt(&self) -> &str {
        &self.message
    }

    fn hint(&self) -> Option<&str> {
        Some(&self.hint)
    }

    fn validate(&mut self) -> Result<Validation, Self::ValidateErr> {
        match self.input.value().unwrap_or(self.list.list.default) {
            'h' => {
                self.expanded = true;
                self.input.set_value(None);
                self.list.list.selected = None;
                Ok(Validation::Continue)
            }
            _ => Ok(Validation::Finish),
        }
    }

    fn finish(self) -> Self::Output {
        let c = self.input.value().unwrap_or(self.list.list.default);
        self.finish_with(c)
    }

    fn has_default(&self) -> bool {
        self.list.list.default != 'h'
    }

    fn finish_default(self) -> Self::Output {
        let c = self.list.list.default;
        self.finish_with(c)
    }
}

const ANSWER_PROMPT: &[u8] = b"  Answer: ";

impl<F: Fn(char) -> Option<char>> ui::Widget for ExpandPrompt<'_, F> {
    fn render<W: std::io::Write>(&mut self, max_width: usize, w: &mut W) -> crossterm::Result<()> {
        if self.expanded {
            let max_width = terminal::size()?.0 as usize - ANSWER_PROMPT.len();
            self.list.render(max_width, w)?;
            w.write_all(ANSWER_PROMPT)?;
            self.input.render(max_width, w)
        } else {
            self.input.render(max_width, w)?;

            if let Some(key) = self.input.value() {
                let name = &self
                    .list
                    .list
                    .choices
                    .choices
                    .iter()
                    .filter_map(|choice| match choice {
                        Choice::Choice(choice) => Some(choice),
                        _ => None,
                    })
                    .find(|item| item.key == key)
                    .map(|item| &*item.name)
                    .unwrap_or("Help, list all options");

                queue!(w, cursor::MoveToNextLine(1))?;

                write!(w, "{} {}", ">>".dark_cyan(), name)?;
            }

            Ok(())
        }
    }

    fn height(&self) -> usize {
        if self.expanded {
            self.list.height() + 1
        } else if self.input.value().is_some() {
            self.input.height() + 1
        } else {
            self.input.height()
        }
    }

    fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        if self.input.handle_key(key) {
            self.list.list.selected = self.input.value();
            true
        } else if self.expanded {
            self.list.handle_key(key)
        } else {
            false
        }
    }

    fn cursor_pos(&self, prompt_len: u16) -> (u16, u16) {
        if self.expanded {
            let w = self.input.cursor_pos(ANSWER_PROMPT.len() as u16).0;
            (w, self.height() as u16)
        } else {
            self.input.cursor_pos(prompt_len)
        }
    }
}

thread_local! {
    static HELP_CHOICE: ExpandItem = ExpandItem {
        key: 'h',
        name: "Help, list all options".into(),
    };
}

impl widgets::List for Expand<'_> {
    fn render_item<W: std::io::Write>(
        &mut self,
        index: usize,
        _: bool,
        max_width: usize,
        w: &mut W,
    ) -> crossterm::Result<()> {
        if index == self.choices.len() {
            return HELP_CHOICE.with(|h| self.render_choice(h, max_width, w));
        }

        match &self.choices[index] {
            Choice::Choice(item) => self.render_choice(item, max_width, w),
            Choice::Separator(s) => {
                queue!(w, SetForegroundColor(Color::DarkGrey))?;
                w.write_all(b"   ")?;
                super::get_sep_str(s).render(max_width - 3, w)?;
                queue!(w, ResetColor)
            }
        }
    }

    fn is_selectable(&self, _: usize) -> bool {
        true
    }

    fn len(&self) -> usize {
        self.choices.len() + 1
    }

    fn page_size(&self) -> usize {
        self.choices.page_size()
    }

    fn should_loop(&self) -> bool {
        self.choices.should_loop()
    }
}

impl Expand<'_> {
    fn render_choice<W: std::io::Write>(
        &self,
        item: &ExpandItem,
        max_width: usize,
        w: &mut W,
    ) -> crossterm::Result<()> {
        let hovered = self.selected.map(|c| c == item.key).unwrap_or(false);

        if hovered {
            queue!(w, SetForegroundColor(Color::DarkCyan))?;
        }

        write!(w, "  {}) ", item.key)?;
        item.name.as_str().render(max_width - 5, w)?;

        if hovered {
            queue!(w, ResetColor)?;
        }

        Ok(())
    }

    pub fn ask<W: std::io::Write>(
        mut self,
        message: String,
        answers: &Answers,
        w: &mut W,
    ) -> error::Result<Answer> {
        let choices = self
            .choices
            .choices
            .iter()
            .filter_map(|choice| match choice {
                Choice::Choice(choice) => Some(choice.key.to_ascii_lowercase()),
                _ => None,
            })
            .chain(std::iter::once('h'))
            .collect::<String>();

        let hint = {
            let mut s = String::with_capacity(2 + choices.len());
            s.push('(');
            s.extend(choices.chars().map(|c| {
                if c == self.default {
                    c.to_ascii_uppercase()
                } else {
                    c
                }
            }));
            s.push(')');
            s
        };

        let transformer = self.transformer.take();

        let ans = ui::Input::new(ExpandPrompt {
            message,
            input: widgets::CharInput::new(|c| {
                let c = c.to_ascii_lowercase();
                choices.contains(c).then(|| c)
            }),
            list: widgets::ListPicker::new(self),
            hint,
            expanded: false,
        })
        .run(w)?;

        match transformer {
            Some(transformer) => transformer(&ans, answers, w)?,
            None => writeln!(w, "{}", ans.name.as_str().dark_cyan())?,
        }

        Ok(Answer::ExpandItem(ans))
    }
}

pub struct ExpandBuilder<'m, 'w, 't> {
    opts: Options<'m, 'w>,
    expand: Expand<'t>,
    keys: HashSet<char>,
}

impl<'m, 'w, 't> ExpandBuilder<'m, 'w, 't> {
    pub fn default(mut self, default: char) -> Self {
        self.expand.default = default;
        self
    }

    pub fn separator<I: Into<String>>(mut self, text: I) -> Self {
        self.expand
            .choices
            .choices
            .push(Choice::Separator(Some(text.into())));
        self
    }

    pub fn default_separator(mut self) -> Self {
        self.expand.choices.choices.push(Choice::Separator(None));
        self
    }

    pub fn choice(mut self, mut key: char, name: String) -> Self {
        key = key.to_ascii_lowercase();
        if key == 'h' {
            panic!("Reserved key 'h'");
        }
        if self.keys.contains(&key) {
            panic!("Duplicate key '{}'", key);
        }

        self.keys.insert(key);

        self.expand
            .choices
            .choices
            .push(Choice::Choice(ExpandItem { key, name }));

        self
    }

    pub fn choices<I, T>(mut self, choices: I) -> Self
    where
        T: Into<Choice<ExpandItem>>,
        I: IntoIterator<Item = T>,
    {
        let Self {
            ref mut keys,
            ref mut expand,
            ..
        } = self;
        expand
            .choices
            .choices
            .extend(choices.into_iter().map(Into::into).inspect(|choice| {
                if let Choice::Choice(c) = choice {
                    let key = c.key.to_ascii_lowercase();
                    if key == 'h' {
                        panic!("Reserved key 'h'");
                    }
                    if keys.contains(&key) {
                        panic!("Duplicate key '{}'", key);
                    }
                    keys.insert(key);
                }
            }));
        self
    }

    pub fn page_size(mut self, page_size: usize) -> Self {
        self.expand.choices.set_page_size(page_size);
        self
    }

    pub fn should_loop(mut self, should_loop: bool) -> Self {
        self.expand.choices.set_should_loop(should_loop);
        self
    }

    pub fn build(self) -> super::Question<'m, 'w, 'static, 'static, 't> {
        super::Question::new(self.opts, super::QuestionKind::Expand(self.expand))
    }
}

impl<'m, 'w, 't> From<ExpandBuilder<'m, 'w, 't>> for super::Question<'m, 'w, 'static, 'static, 't> {
    fn from(builder: ExpandBuilder<'m, 'w, 't>) -> Self {
        builder.build()
    }
}

crate::impl_options_builder!(ExpandBuilder<'t>; (this, opts) => {
    ExpandBuilder {
        opts,
        expand: this.expand,
        keys: this.keys,
    }
});

crate::impl_transformer_builder!(ExpandBuilder<'m, 'w, t> ExpandItem; (this, transformer) => {
    ExpandBuilder {
        opts: this.opts,
        keys: this.keys,
        expand: Expand {
            transformer,
            choices: this.expand.choices,
            default: this.expand.default,
            selected: this.expand.selected,
        }
    }
});

impl super::Question<'static, 'static, 'static, 'static, 'static> {
    pub fn expand<N: Into<String>>(name: N) -> ExpandBuilder<'static, 'static, 'static> {
        ExpandBuilder {
            opts: Options::new(name.into()),
            expand: Default::default(),
            keys: Default::default(),
        }
    }
}
