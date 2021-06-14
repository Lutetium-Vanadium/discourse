use std::ops::{Deref, DerefMut};

use super::{Validation, Widget};
use crate::{
    backend::{Backend, ClearType, MoveDirection, Size},
    error,
    events::{KeyCode, KeyEvent, KeyModifiers},
    layout::Layout,
    style::Stylize,
};

/// This trait should be implemented by all 'root' widgets.
///
/// It provides the functionality specifically required only by the main controlling widget. For the
/// trait required for general rendering to terminal, see [`Widget`].
pub trait Prompt: Widget {
    /// The error type returned by validate. It **must** be only one line long.
    type ValidateErr: Widget;

    /// The output type returned by [`Input::run`]
    type Output;

    /// Determine whether the prompt state is ready to be submitted. It is called whenever the use
    /// presses the enter key.
    #[allow(unused_variables)]
    fn validate(&mut self) -> Result<Validation, Self::ValidateErr> {
        Ok(Validation::Finish)
    }
    /// The value to return from [`Input::run`]. This will only be called once validation returns
    /// [`Validation::Finish`];
    fn finish(self) -> Self::Output;

    /// The prompt has some default value that can be returned.
    fn has_default(&self) -> bool;
    /// The default value to be returned. It will only be called when has_default is true and the
    /// user presses escape.
    fn finish_default(self) -> Self::Output
    where
        Self: Sized,
    {
        unreachable!();
    }
}

/// The ui runner. It renders and processes events with the help of a type that implements [`Prompt`]
///
/// See [`run`](Input::run) for more information
pub struct Input<P, B: Backend> {
    prompt: P,
    backend: TerminalState<B>,
    base_row: u16,
    size: Size,
}

impl<P: Prompt, B: Backend> Input<P, B> {
    fn layout(&self) -> Layout {
        Layout::new(0, self.size).with_offset(0, self.base_row)
    }

    fn init(&mut self) -> error::Result<()> {
        self.backend.init()?;
        self.base_row = self.backend.get_cursor_pos()?.1;
        self.render()
    }

    fn adjust_scrollback(&mut self, height: u16) -> error::Result<u16> {
        let th = self.size.height;

        let mut base_row = self.base_row;

        if self.base_row >= th - height {
            let dist = self.base_row + height - th + 1;
            base_row -= dist;
            self.backend.scroll(-(dist as i16))?;
            self.backend.move_cursor(MoveDirection::Up(dist))?;
        }

        Ok(base_row)
    }

    fn flush(&mut self) -> error::Result<()> {
        if !self.backend.hide_cursor {
            let (dcw, dch) = self.prompt.cursor_pos(self.layout());
            self.backend.move_cursor_to(dcw, self.base_row + dch)?;
        }
        self.backend.flush().map_err(Into::into)
    }

    fn render(&mut self) -> error::Result<()> {
        self.size = self.backend.size()?;
        let height = self.prompt.height(&mut self.layout()).saturating_sub(1);
        self.base_row = self.adjust_scrollback(height)?;
        self.clear()?;

        self.prompt.render(&mut self.layout(), &mut *self.backend)?;

        self.flush()
    }

    fn clear(&mut self) -> error::Result<()> {
        self.backend.move_cursor_to(0, self.base_row)?;
        self.backend.clear(ClearType::FromCursorDown)
    }

    fn goto_last_line(&mut self, height: u16) -> error::Result<()> {
        self.base_row = self.adjust_scrollback(height)?;
        self.backend.move_cursor_to(0, self.base_row + height)
    }

    fn print_error(&mut self, mut e: P::ValidateErr) -> error::Result<()> {
        self.size = self.backend.size()?;
        let height = self.prompt.height(&mut self.layout());
        self.goto_last_line(height)?;

        self.backend.write_styled(&crate::symbols::CROSS.red())?;
        self.backend.write_all(b" ")?;

        let mut layout =
            Layout::new(2, self.size).with_offset(0, self.base_row + height);

        self.adjust_scrollback(
            height + e.height(&mut layout.clone()).saturating_sub(1),
        )?;
        e.render(&mut layout, &mut *self.backend)?;

        self.flush()
    }

    fn exit(&mut self) -> error::Result<()> {
        self.size = self.backend.size()?;
        let height = self.prompt.height(&mut self.layout());
        self.goto_last_line(height)?;
        self.backend.reset()?;
        Ok(())
    }

    #[inline]
    fn finish(mut self, pressed_enter: bool) -> error::Result<P::Output> {
        self.clear()?;
        self.backend.reset()?;

        if pressed_enter {
            Ok(self.prompt.finish())
        } else {
            Ok(self.prompt.finish_default())
        }
    }

    /// Run the ui on the given writer. It will return when the user presses `Enter` or `Escape`
    /// based on the [`Prompt`] implementation.
    pub fn run<E>(mut self, events: &mut E) -> error::Result<P::Output>
    where
        E: Iterator<Item = error::Result<KeyEvent>>,
    {
        self.init()?;

        loop {
            let e = events.next().unwrap()?;

            let key_handled = match e.code {
                KeyCode::Char('c')
                    if e.modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    self.exit()?;
                    return Err(error::ErrorKind::Interrupted);
                }
                KeyCode::Null => {
                    self.exit()?;
                    return Err(error::ErrorKind::Eof);
                }
                KeyCode::Esc if self.prompt.has_default() => {
                    return self.finish(false);
                }

                KeyCode::Enter => match self.prompt.validate() {
                    Ok(Validation::Finish) => {
                        return self.finish(true);
                    }
                    Ok(Validation::Continue) => true,
                    Err(e) => {
                        self.print_error(e)?;

                        continue;
                    }
                },
                _ => self.prompt.handle_key(e),
            };

            if key_handled {
                self.render()?;
            }
        }
    }
}

impl<P, B: Backend> Input<P, B> {
    #[allow(clippy::new_ret_no_self)]
    /// Creates a new Input
    pub fn new(prompt: P, backend: &mut B) -> Input<P, &mut B> {
        // The method doesn't return self directly, as its always used with a `&mut B`,
        // and this tells the compiler that it doesn't need to consume the `&mut B`, but
        // once the Input goes out of scope, it can be used again
        Input {
            prompt,
            backend: TerminalState::new(backend, false),
            base_row: 0,
            size: Size::default(),
        }
    }

    /// Hides the cursor while running the input
    pub fn hide_cursor(mut self) -> Self {
        self.backend.hide_cursor = true;
        self
    }
}

struct TerminalState<B: Backend> {
    backend: B,
    hide_cursor: bool,
    enabled: bool,
}

impl<B: Backend> TerminalState<B> {
    fn new(backend: B, hide_cursor: bool) -> Self {
        Self {
            backend,
            enabled: false,
            hide_cursor,
        }
    }

    fn init(&mut self) -> error::Result<()> {
        self.enabled = true;
        if self.hide_cursor {
            self.backend.hide_cursor()?;
        }
        self.backend.enable_raw_mode()
    }

    fn reset(&mut self) -> error::Result<()> {
        self.enabled = false;
        if self.hide_cursor {
            self.backend.show_cursor()?;
        }
        self.backend.disable_raw_mode()
    }
}

impl<B: Backend> Drop for TerminalState<B> {
    fn drop(&mut self) {
        if self.enabled {
            let _ = self.reset();
        }
    }
}

impl<B: Backend> Deref for TerminalState<B> {
    type Target = B;

    fn deref(&self) -> &Self::Target {
        &self.backend
    }
}

impl<B: Backend> DerefMut for TerminalState<B> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.backend
    }
}