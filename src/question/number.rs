use std::{fmt::Write, io};

use ui::{
    backend::Backend,
    events::{EventIterator, KeyCode, KeyEvent},
    style::Color,
    widgets, Prompt, Validation, Widget,
};

use super::{Filter, Options, TransformByVal as Transform, ValidateByVal as Validate};
use crate::{Answer, Answers};

#[derive(Debug, Default)]
pub(super) struct Float<'a> {
    default: Option<f64>,
    filter: Filter<'a, f64>,
    validate: Validate<'a, f64>,
    transform: Transform<'a, f64>,
}

#[derive(Debug, Default)]
pub(super) struct Int<'a> {
    default: Option<i64>,
    filter: Filter<'a, i64>,
    validate: Validate<'a, i64>,
    transform: Transform<'a, i64>,
}

impl Int<'_> {
    fn write<B: Backend>(i: i64, b: &mut B) -> io::Result<()> {
        b.set_fg(Color::Cyan)?;
        write!(b, "{}", i)?;
        b.set_fg(Color::Reset)
    }

    fn delta(i: i64, delta: i64) -> i64 {
        i.wrapping_add(delta)
    }

    fn filter_map(c: char) -> Option<char> {
        if c.is_digit(10) || c == '-' || c == '+' {
            Some(c)
        } else {
            None
        }
    }
}

impl Float<'_> {
    fn write<B: Backend>(f: f64, b: &mut B) -> io::Result<()> {
        b.set_fg(Color::Cyan)?;
        if f.log10().abs() > 19.0 {
            write!(b, "{:e}", f)?;
        } else {
            write!(b, "{}", f)?;
        }
        b.set_fg(Color::Reset)
    }

    fn delta(f: f64, delta: i64) -> f64 {
        f + delta as f64
    }

    fn filter_map(c: char) -> Option<char> {
        if Int::filter_map(c).is_some() || ['.', 'e', 'E', 'i', 'n', 'f'].contains(&c) {
            Some(c)
        } else {
            None
        }
    }
}

macro_rules! impl_number_prompt {
    ($prompt_name:ident, $type:ident, $inner_ty:ty) => {
        struct $prompt_name<'n, 'a> {
            prompt: widgets::Prompt<&'a str, String>,
            number: $type<'n>,
            input: widgets::StringInput,
            answers: &'a Answers,
        }

        impl $prompt_name<'_, '_> {
            fn parse(&self) -> Result<$inner_ty, String> {
                self.input
                    .value()
                    .parse::<$inner_ty>()
                    .map_err(|e| e.to_string())
            }
        }

        impl Widget for $prompt_name<'_, '_> {
            fn render<B: Backend>(
                &mut self,
                layout: &mut ui::layout::Layout,
                b: &mut B,
            ) -> io::Result<()> {
                self.prompt.render(layout, b)?;
                self.input.render(layout, b)
            }

            fn height(&mut self, layout: &mut ui::layout::Layout) -> u16 {
                self.prompt.height(layout) + self.input.height(layout) - 1
            }

            fn handle_key(&mut self, key: KeyEvent) -> bool {
                if self.input.handle_key(key) {
                    return true;
                }

                let n = match (key.code, self.parse()) {
                    (KeyCode::PageUp, Ok(n)) => $type::delta(n, 10),
                    (KeyCode::PageDown, Ok(n)) => $type::delta(n, -10),
                    (KeyCode::Up, Ok(n)) => $type::delta(n, 1),
                    (KeyCode::Down, Ok(n)) => $type::delta(n, -1),
                    _ => return false,
                };

                self.input.replace_with(|mut s| {
                    s.clear();
                    write!(s, "{}", n).expect("Failed to write number to the string");
                    s
                });
                true
            }

            fn cursor_pos(&mut self, layout: ui::layout::Layout) -> (u16, u16) {
                self.input
                    .cursor_pos(layout.with_cursor_pos(self.prompt.cursor_pos(layout)))
            }
        }

        impl Prompt for $prompt_name<'_, '_> {
            type ValidateErr = widgets::Text<String>;
            type Output = $inner_ty;

            fn validate(&mut self) -> Result<Validation, Self::ValidateErr> {
                if self.input.value().is_empty() && self.number.default.is_some() {
                    return Ok(Validation::Finish);
                }
                let n = self.parse()?;

                if let Validate::Sync(ref mut validate) = self.number.validate {
                    validate(n, self.answers)?;
                }

                Ok(Validation::Finish)
            }

            fn finish(self) -> Self::Output {
                let n = match self.number.default {
                    Some(default) if self.input.value().is_empty() => default,
                    _ => self
                        .parse()
                        .expect("Validation would fail if number cannot be parsed"),
                };

                match self.number.filter {
                    Filter::Sync(filter) => filter(n, self.answers),
                    _ => n,
                }
            }
        }
    };
}

impl_number_prompt!(IntPrompt, Int, i64);
impl_number_prompt!(FloatPrompt, Float, f64);

macro_rules! impl_ask {
    ($t:ident, $prompt_name:ident) => {
        impl<'n> $t<'n> {
            fn into_prompt<'a>(
                self,
                message: &'a str,
                answers: &'a Answers,
            ) -> $prompt_name<'n, 'a> {
                $prompt_name {
                    prompt: widgets::Prompt::new(message)
                        .with_optional_hint(self.default.as_ref().map(ToString::to_string)),
                    input: widgets::StringInput::with_filter_map(Self::filter_map),
                    number: self,
                    answers,
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

                let ans = ui::Input::new(self.into_prompt(&message, answers), b).run(events)?;

                crate::write_final!(transform, message, ans, answers, b, Self::write(ans, b)?);

                Ok(Answer::$t(ans))
            }
        }
    };
}

impl_ask!(Int, IntPrompt);
impl_ask!(Float, FloatPrompt);

macro_rules! builder {
    ($(#[$meta:meta])* struct $builder_name:ident : $type:ident -> $inner_ty:ty, $litral:expr;
     declare = $declare:expr;
     default = $default:expr;
     filter = $filter:expr;
     validate = $validate:expr;
     ) => {
        $(#[$meta])*
        #[derive(Debug)]
        pub struct $builder_name<'a> {
            opts: Options<'a>,
            inner: $type<'a>,
        }

        impl<'a> $builder_name<'a> {
            pub(crate) fn new(name: String) -> Self {
                $builder_name {
                    opts: Options::new(name),
                    inner: Default::default(),
                }
            }

            crate::impl_options_builder! {
            message
            /// # Examples
            ///
            /// ```
            /// use discourse::Question;
            ///
            #[doc = $declare]
            ///     .message("Please enter a number")
            ///     .build();
            /// ```

            when
            /// # Examples
            ///
            /// ```
            /// use discourse::{Question, Answers};
            ///
            #[doc = $declare]
            ///     .when(|previous_answers: &Answers| match previous_answers.get("ask_number") {
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
            #[doc = $declare]
            ///     .ask_if_answered(true)
            ///     .build();
            /// ```
            }

            /// Set a default value
            ///
            /// If the input text is empty, the `default` is taken as the answer.
            ///
            /// If `default` is used, validation is skipped, but `filter` is still called.
            ///
            /// # Examples
            ///
            /// ```
            /// use discourse::Question;
            ///
            #[doc = $declare]
            #[doc = $default]
            ///     .build();
            /// ```
            pub fn default(mut self, default: $inner_ty) -> Self {
                self.inner.default = Some(default);
                self
            }

            crate::impl_filter_builder! {
            /// # Examples
            ///
            /// ```
            /// use discourse::Question;
            ///
            #[doc = $declare]
            #[doc = $filter]
            ///     .build();
            /// ```
            $inner_ty; inner
            }

            crate::impl_validate_builder! {
            /// # Examples
            ///
            /// ```
            /// use discourse::Question;
            ///
            #[doc = $declare]
            ///     .validate(|n, previous_answers| {
            #[doc = $validate]
            ///             Ok(())
            ///         } else {
            ///             Err("Please enter a positive number".to_owned())
            ///         }
            ///     })
            ///     .build();
            /// ```
            by val $inner_ty; inner
            }

            crate::impl_transform_builder! {
            /// # Examples
            ///
            /// ```
            /// use discourse::Question;
            ///
            #[doc = $declare]
            ///     .transform(|n, previous_answers, backend| {
            ///         write!(backend, "{:e}", n)
            ///     })
            ///     .build();
            /// ```
            by val $inner_ty; inner
            }

            /// Consumes the builder returning a [`Question`]
            ///
            /// [`Question`]: crate::question::Question
            pub fn build(self) -> super::Question<'a> {
                super::Question::new(self.opts, super::QuestionKind::$type(self.inner))
            }
        }

        impl<'a> From<$builder_name<'a>> for super::Question<'a> {
            /// Consumes the builder returning a [`Question`]
            ///
            /// [`Question`]: crate::question::Question
            fn from(builder: $builder_name<'a>) -> Self {
                builder.build()
            }
        }
    };
}

builder! {
/// The builder for an [`int`] prompt.
///
/// The number is parsed using [`from_str`].
///
/// See the various methods for more details on each available option.
///
/// # Examples
///
/// ```
/// use discourse::Question;
///
/// let int = Question::int("age")
///     .message("What is your age?")
///     .validate(|age, previous_answers| {
///         if age > 0 && age < 130 {
///             Ok(())
///         } else {
///             Err(format!("You cannot be {} years old!", age))
///         }
///     })
///     .build();
/// ```
///
/// [`from_str`]: https://doc.rust-lang.org/std/primitive.i64.html#method.from_str
/// [`int`]: crate::question::Question::int
struct IntBuilder: Int -> i64, 10;
declare  = r#"let question = Question::int("int")"#;
default  = "    .default(10)";
filter   = "    .filter(|n, previous_answers| n + 10)";
validate = "        if n.is_positive() {";
}

builder! {
/// The builder for a [`float`] prompt.
///
/// The number is parsed using [`from_str`], but cannot be `NaN`.
///
/// See the various methods for more details on each available option.
///
/// # Examples
///
/// ```
/// use discourse::Question;
///
/// let float = Question::float("number")
///     .message("What is your favourite number?")
///     .validate(|num, previous_answers| {
///         if num.is_finite() {
///             Ok(())
///         } else {
///             Err("Please enter a finite number".to_owned())
///         }
///     })
///     .build();
/// ```
///
/// [`float`]: crate::question::Question::float
/// [`from_str`]: https://doc.rust-lang.org/std/primitive.f64.html#method.from_str
struct FloatBuilder: Float -> f64, 10.0;
declare  = r#"let question = Question::float("float")"#;
default  = "    .default(10.0)";
filter   = "    .filter(|n, previous_answers| (n * 10000.0).round() / 10000.0)";
validate = "        if n.is_sign_positive() {";
}

macro_rules! test_numbers {
    (mod $mod_name:ident { $prompt_name:ident, $default:expr }) => {
        #[cfg(test)]
        mod $mod_name {
            use ui::{backend::TestBackend, layout::Layout};

            use super::*;

            #[test]
            fn test_render() {
                let size = (50, 20).into();
                let base_layout = Layout::new(5, size);
                let answers = Answers::default();

                let defaults = [(None, 17), (Some($default), 21)];

                let mut backend = TestBackend::new_with_layout(size, base_layout);

                for &(default, line_offset) in defaults.iter() {
                    let mut prompt = $prompt_name {
                        default,
                        ..Default::default()
                    }
                    .into_prompt("message", &answers);

                    let base_name = default.map(|_| "default").unwrap_or("no_default");

                    let mut layout = base_layout;
                    backend.reset_with_layout(layout);
                    assert!(prompt.render(&mut layout, &mut backend).is_ok());
                    assert_eq!(layout, base_layout.with_line_offset(line_offset));
                    ui::assert_backend_snapshot!(format!("{}-1", base_name), backend);

                    prompt.input.set_value("3".repeat(50));

                    layout = base_layout;
                    backend.reset_with_layout(layout);
                    assert!(prompt.render(&mut layout, &mut backend).is_ok());
                    assert_eq!(
                        layout,
                        base_layout.with_offset(0, 1).with_line_offset(line_offset)
                    );
                    ui::assert_backend_snapshot!(format!("{}-2", base_name), backend);
                }
            }

            #[test]
            fn test_height() {
                let size = (50, 20).into();
                let base_layout = Layout::new(5, size);
                let answers = Answers::default();

                let defaults = [(None, 17), (Some($default), 21)];

                for &(default, line_offset) in defaults.iter() {
                    let mut prompt = $prompt_name {
                        default,
                        ..Default::default()
                    }
                    .into_prompt("message", &answers);

                    let mut layout = base_layout;

                    assert_eq!(prompt.height(&mut layout), 1);
                    assert_eq!(layout, base_layout.with_line_offset(line_offset));
                    layout = base_layout;

                    prompt.input.set_value("3".repeat(50));
                    assert_eq!(prompt.height(&mut layout), 2);
                    assert_eq!(
                        layout,
                        base_layout.with_offset(0, 1).with_line_offset(line_offset)
                    );
                }
            }

            #[test]
            fn test_cursor_pos() {
                let size = (50, 20).into();
                let layout = Layout::new(5, size);
                let answers = Answers::default();

                let defaults = [(None, 17), (Some($default), 21)];

                for &(default, line_offset) in defaults.iter() {
                    let mut prompt = $prompt_name {
                        default,
                        ..Default::default()
                    }
                    .into_prompt("message", &answers);

                    assert_eq!(prompt.cursor_pos(layout), (line_offset, 0));

                    prompt.input.set_value("3".repeat(50));
                    prompt.input.set_at(50);
                    assert_eq!(prompt.cursor_pos(layout), (line_offset, 1));
                }
            }
        }
    };
}

test_numbers!(mod int_tests { Int, 333 });
test_numbers!(mod float_tests { Float, 3.3 });
