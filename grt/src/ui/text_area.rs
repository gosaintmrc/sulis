//  This file is part of Sulis, a turn based RPG written in Rust.
//  Copyright 2018 Jared Stephen
//
//  Sulis is free software: you can redistribute it and/or modify
//  it under the terms of the GNU General Public License as published by
//  the Free Software Foundation, either version 3 of the License, or
//  (at your option) any later version.
//
//  Sulis is distributed in the hope that it will be useful,
//  but WITHOUT ANY WARRANTY; without even the implied warranty of
//  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
//  GNU General Public License for more details.
//
//  You should have received a copy of the GNU General Public License
//  along with Sulis.  If not, see <http://www.gnu.org/licenses/>

use std::rc::Rc;

use ui::{MarkupRenderer, Widget, WidgetKind};
use io::GraphicsRenderer;
use util::Point;

pub struct TextArea {
    pub text: Option<String>,
}

impl TextArea {
    pub fn empty() -> Rc<TextArea> {
        Rc::new(TextArea {
            text: None,
        })
    }

    pub fn new(text: &str) -> Rc<TextArea> {
        Rc::new(TextArea {
            text: Some(text.to_string()),
        })
    }
}

impl WidgetKind for TextArea {
    fn get_name(&self) -> &str {
        "text_area"
    }

    fn layout(&self, widget: &mut Widget) {
        if let Some(ref text) = self.text {
            widget.state.add_text_arg("0", text);
        }

        widget.do_base_layout();

        if let Some(ref font) = widget.state.font {
            widget.state.text_renderer = Some(Box::new(
                    MarkupRenderer::new(font, widget.state.inner_size.width)
                    ));
        }
    }

    fn draw_graphics_mode(&self, renderer: &mut GraphicsRenderer, _pixel_size: Point,
                          widget: &Widget, _millis: u32) {
        let font_rend = match &widget.state.text_renderer {
            &None => return,
            &Some(ref renderer) => renderer,
        };

        let x = widget.state.inner_left() as f32;
        let y = widget.state.inner_top() as f32;
        font_rend.render(renderer, &widget.state.text, x, y, &widget.state.text_params);
    }
}
