use std::io;

use ui::{
    backend::Backend,
    events::{EventIterator, KeyEvent},
    style::{Color, Stylize},
    widgets::{self, List, Text},
    Prompt, Validation, Widget,
};

use super::{Choice, Options, Transform};
use crate::{Answer, Answers, ListItem};

#[cfg(test)]
mod tests;

// Kind of a bad name
#[derive(Debug, Default)]
pub(super) struct RawSelect<'a> {
    choices: super::ChoiceList<(usize, Text<String>)>,
    transform: Transform<'a, ListItem>,
}

struct RawSelectPrompt<'a> {
    prompt: widgets::Prompt<&'a str>,
    select: widgets::Select<RawSelect<'a>>,
    input: widgets::StringInput,
}

impl RawSelectPrompt<'_> {
    fn finish_index(self, index: usize) -> ListItem {
        ListItem {
            index,
            name: self
                .select
                .into_inner()
                .choices
                .choices
                .swap_remove(index)
                .unwrap_choice()
                .1
                .text,
        }
    }
}

impl Prompt for RawSelectPrompt<'_> {
    type ValidateErr = &'static str;
    type Output = ListItem;

    fn validate(&mut self) -> Result<Validation, Self::ValidateErr> {
        if self.select.get_at() >= self.select.list.len() {
            Err("Please enter a valid choice")
        } else {
            Ok(Validation::Finish)
        }
    }

    fn finish(self) -> Self::Output {
        let index = self.select.get_at();
        self.finish_index(index)
    }
}

const ANSWER_PROMPT: &[u8] = b"  Answer: ";

impl Widget for RawSelectPrompt<'_> {
    fn render<B: Backend>(&mut self, layout: &mut ui::layout::Layout, b: &mut B) -> io::Result<()> {
        self.prompt.render(layout, b)?;
        self.select.render(layout, b)?;
        b.write_all(ANSWER_PROMPT)?;
        layout.line_offset += ANSWER_PROMPT.len() as u16;
        self.input.render(layout, b)
    }

    fn height(&mut self, layout: &mut ui::layout::Layout) -> u16 {
        // We don't need to add 1 for the answer prompt because this will over count by one
        let height = self.prompt.height(layout) + self.select.height(layout);
        layout.line_offset = ANSWER_PROMPT.len() as u16;
        height + self.input.height(layout) - 1
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if self.input.handle_key(key) {
            if let Ok(n) = self.input.value().parse::<usize>() {
                if n <= self.select.list.len() && n > 0 {
                    let pos = self.select.list.choices.choices[(n - 1)..]
                        .iter()
                        .position(|choice| matches!(choice, Choice::Choice((i, _)) if *i == n));

                    if let Some(pos) = pos {
                        self.select.set_at(pos + n - 1);
                        return true;
                    }
                }
            }

            self.select.set_at(self.select.list.len() + 1);
            true
        } else if self.select.handle_key(key) {
            let at = self.select.get_at();
            let index = self.select.list.choices[at].as_ref().unwrap_choice().0;
            self.input.set_value(index.to_string());
            true
        } else {
            false
        }
    }

    fn cursor_pos(&mut self, mut layout: ui::layout::Layout) -> (u16, u16) {
        let w = self
            .input
            .cursor_pos(layout.with_line_offset(ANSWER_PROMPT.len() as u16))
            .0;
        (w, self.height(&mut layout) - 1)
    }
}

impl widgets::List for RawSelect<'_> {
    fn render_item<B: Backend>(
        &mut self,
        index: usize,
        hovered: bool,
        mut layout: ui::layout::Layout,
        b: &mut B,
    ) -> io::Result<()> {
        match &mut self.choices[index] {
            &mut Choice::Choice((index, ref mut name)) => {
                if hovered {
                    b.set_fg(Color::Cyan)?;
                }

                write!(b, "  {}) ", index)?;

                layout.offset_x += (index as f64).log10() as u16 + 5;
                name.render(&mut layout, b)?;

                if hovered {
                    b.set_fg(Color::Reset)?;
                }
            }
            separator => {
                b.set_fg(Color::DarkGrey)?;
                b.write_all(b"   ")?;
                super::get_sep_str(separator).render(&mut layout.with_line_offset(3), b)?;
                b.set_fg(Color::Reset)?;
            }
        }

        Ok(())
    }

    fn is_selectable(&self, index: usize) -> bool {
        !self.choices[index].is_separator()
    }

    fn height_at(&mut self, index: usize, mut layout: ui::layout::Layout) -> u16 {
        match self.choices[index] {
            Choice::Choice((index, ref mut c)) => {
                layout.offset_x += (index as f64).log10() as u16 + 5;
                c.height(&mut layout)
            }
            _ => 1,
        }
    }

    fn len(&self) -> usize {
        self.choices.len()
    }

    fn page_size(&self) -> usize {
        self.choices.page_size()
    }

    fn should_loop(&self) -> bool {
        self.choices.should_loop()
    }
}

impl<'a> RawSelect<'a> {
    fn into_prompt(self, message: &'a str) -> RawSelectPrompt<'a> {
        let mut select = widgets::Select::new(self);
        if let Some(default) = select.list.choices.default() {
            select.set_at(default);
        }

        RawSelectPrompt {
            input: widgets::StringInput::with_filter_map(|c| {
                if c.is_digit(10) {
                    Some(c)
                } else {
                    None
                }
            }),
            select,
            prompt: widgets::Prompt::new(&message),
        }
    }

    pub(crate) fn ask<B: Backend, E: EventIterator>(
        mut self,
        message: String,
        answers: &Answers,
        b: &mut B,
        events: &mut E,
    ) -> ui::Result<Answer> {
        let transform = self.transform.take();

        let ans = ui::Input::new(self.into_prompt(&message), b).run(events)?;

        crate::write_final!(
            transform,
            message,
            &ans,
            answers,
            b,
            b.write_styled(
                &ans.name
                    .lines()
                    .next()
                    .expect("There must be at least one line in a `str`")
                    .cyan()
            )?
        );

        Ok(Answer::ListItem(ans))
    }
}

/// The builder for a [`raw_select`] prompt.
///
/// See the various methods for more details on each available option.
///
/// # Examples
///
/// ```
/// use discourse::{Question, DefaultSeparator};
///
/// let raw_select = Question::raw_select("theme")
///     .message("What do you want to do?")
///     .choices(vec![
///         "Order a pizza".into(),
///         "Make a reservation".into(),
///         DefaultSeparator,
///         "Ask for opening hours".into(),
///         "Contact support".into(),
///         "Talk to the receptionist".into(),
///     ])
///     .build();
/// ```
///
/// [`raw_select`]: crate::question::Question::raw_select
#[derive(Debug)]
pub struct RawSelectBuilder<'a> {
    opts: Options<'a>,
    raw_select: RawSelect<'a>,
    choice_count: usize,
}

impl<'a> RawSelectBuilder<'a> {
    pub(crate) fn new(name: String) -> Self {
        RawSelectBuilder {
            opts: Options::new(name),
            raw_select: Default::default(),
            // It is one indexed for the user
            choice_count: 1,
        }
    }

    crate::impl_options_builder! {
    message
    /// # Examples
    ///
    /// ```
    /// use discourse::Question;
    ///
    /// let raw_select = Question::raw_select("theme")
    ///     .message("What do you want to do?")
    ///     .build();
    /// ```

    when
    /// # Examples
    ///
    /// ```
    /// use discourse::{Question, Answers};
    ///
    /// let raw_select = Question::raw_select("theme")
    ///     .when(|previous_answers: &Answers| match previous_answers.get("use-default-theme") {
    ///         Some(ans) => ans.as_bool().unwrap(),
    ///         None => true,
    ///     })
    ///     .build();
    /// ```

    ask_if_answered
    /// # Examples
    ///
    /// ```
    /// use discourse::{Question, Answers};
    ///
    /// let raw_select = Question::raw_select("theme")
    ///     .ask_if_answered(true)
    ///     .build();
    /// ```
    }

    /// Set a default index for the select
    ///
    /// The given index will be hovered in the beginning.
    ///
    /// If `default` is unspecified, the first [`Choice`] will be hovered.
    ///
    /// # Panics
    ///
    /// If the default given is not a [`Choice`], it will cause a panic on [`build`]
    ///
    /// [`Choice`]: super::Choice
    /// [`build`]: Self::build
    ///
    /// # Examples
    ///
    /// ```
    /// use discourse::{Question, DefaultSeparator};
    ///
    /// let raw_select = Question::raw_select("theme")
    ///     .choices(vec![
    ///         "Order a pizza".into(),
    ///         "Make a reservation".into(),
    ///         DefaultSeparator,
    ///         "Ask for opening hours".into(),
    ///         "Contact support".into(),
    ///         "Talk to the receptionist".into(),
    ///     ])
    ///     .default(1)
    ///     .build();
    /// ```
    pub fn default(mut self, default: usize) -> Self {
        self.raw_select.choices.set_default(default);
        self
    }

    /// The maximum height that can be taken by the list
    ///
    /// If the total height exceeds the page size, the list will be scrollable.
    ///
    /// The `page_size` must be a minimum of 5. If `page_size` is not set, it will default to 15.
    ///
    /// # Panics
    ///
    /// It will panic if the `page_size` is less than 5.
    ///
    /// # Examples
    ///
    /// ```
    /// use discourse::Question;
    ///
    /// let raw_select = Question::raw_select("theme")
    ///     .page_size(10)
    ///     .build();
    /// ```
    pub fn page_size(mut self, page_size: usize) -> Self {
        assert!(page_size >= 5, "page size can be a minimum of 5");

        self.raw_select.choices.set_page_size(page_size);
        self
    }

    /// Whether to wrap around when user gets to the last element.
    ///
    /// This only applies when the list is scrollable, i.e. page size > total height.
    ///
    /// If `should_loop` is not set, it will default to `true`.
    ///
    /// # Examples
    ///
    /// ```
    /// use discourse::Question;
    ///
    /// let raw_select = Question::raw_select("theme")
    ///     .should_loop(false)
    ///     .build();
    /// ```
    pub fn should_loop(mut self, should_loop: bool) -> Self {
        self.raw_select.choices.set_should_loop(should_loop);
        self
    }

    /// Inserts a [`Choice`].
    ///
    /// See [`raw_select`] for more information.
    ///
    /// [`Choice`]: super::Choice::Choice
    /// [`raw_select`]: super::Question::raw_select
    ///
    /// # Examples
    ///
    /// ```
    /// use discourse::Question;
    ///
    /// let raw_select = Question::raw_select("theme")
    ///     .choice("Order a Pizza")
    ///     .build();
    /// ```
    pub fn choice<I: Into<String>>(mut self, choice: I) -> Self {
        self.raw_select.choices.choices.push(Choice::Choice((
            self.choice_count,
            Text::new(choice.into()),
        )));
        self.choice_count += 1;
        self
    }

    /// Inserts a [`Separator`] with the given text
    ///
    /// See [`raw_select`] for more information.
    ///
    /// [`Separator`]: super::Choice::Separator
    /// [`raw_select`]: super::Question::raw_select
    ///
    /// # Examples
    ///
    /// ```
    /// use discourse::Question;
    ///
    /// let raw_select = Question::raw_select("theme")
    ///     .separator("-- custom separator text --")
    ///     .build();
    /// ```
    pub fn separator<I: Into<String>>(mut self, text: I) -> Self {
        self.raw_select
            .choices
            .choices
            .push(Choice::Separator(text.into()));
        self
    }

    /// Inserts a [`DefaultSeparator`]
    ///
    /// See [`raw_select`] for more information.
    ///
    /// [`DefaultSeparator`]: super::Choice::DefaultSeparator
    /// [`raw_select`]: super::Question::raw_select
    ///
    /// # Examples
    ///
    /// ```
    /// use discourse::Question;
    ///
    /// let raw_select = Question::raw_select("theme")
    ///     .default_separator()
    ///     .build();
    /// ```
    pub fn default_separator(mut self) -> Self {
        self.raw_select
            .choices
            .choices
            .push(Choice::DefaultSeparator);
        self
    }

    /// Extends the given iterator of [`Choice`]s
    ///
    /// See [`raw_select`] for more information.
    ///
    /// [`Choice`]: super::Choice
    /// [`raw_select`]: super::Question::raw_select
    ///
    /// # Examples
    ///
    /// ```
    /// use discourse::{Question, DefaultSeparator};
    ///
    /// let raw_select = Question::raw_select("theme")
    ///     .choices(vec![
    ///         "Order a pizza".into(),
    ///         "Make a reservation".into(),
    ///         DefaultSeparator,
    ///         "Ask for opening hours".into(),
    ///         "Contact support".into(),
    ///         "Talk to the receptionist".into(),
    ///     ])
    ///     .build();
    /// ```
    pub fn choices<I, T>(mut self, choices: I) -> Self
    where
        T: Into<Choice<String>>,
        I: IntoIterator<Item = T>,
    {
        let choice_count = &mut self.choice_count;
        self.raw_select
            .choices
            .choices
            .extend(choices.into_iter().map(|choice| {
                choice.into().map(|c| {
                    let choice = (*choice_count, Text::new(c));
                    *choice_count += 1;
                    choice
                })
            }));
        self
    }

    crate::impl_transform_builder! {
    /// # Examples
    ///
    /// ```
    /// use discourse::Question;
    ///
    /// let raw_select = Question::raw_select("theme")
    ///     .transform(|choice, previous_answers, backend| {
    ///         write!(backend, "({}) {}", choice.index, choice.name)
    ///     })
    ///     .build();
    /// ```
    ListItem; raw_select
    }

    /// Consumes the builder returning a [`Question`]
    ///
    /// [`Question`]: crate::question::Question
    pub fn build(self) -> super::Question<'a> {
        super::Question::new(self.opts, super::QuestionKind::RawSelect(self.raw_select))
    }
}

impl<'a> From<RawSelectBuilder<'a>> for super::Question<'a> {
    /// Consumes the builder returning a [`Question`]
    ///
    /// [`Question`]: crate::question::Question
    fn from(builder: RawSelectBuilder<'a>) -> Self {
        builder.build()
    }
}
