#[cfg(test)]
mod tests {
    use crate::compiler_frontend::ast::expressions::expression::Expression;
    use crate::compiler_frontend::ast::templates::template::{
        SlotKey, SlotPlaceholder, TemplateAtom, TemplateContent, TemplateSegment,
        TemplateSegmentOrigin,
    };
    use crate::compiler_frontend::ast::templates::template_render_plan::{
        RenderPiece, TemplateRenderPlan,
    };
    use crate::compiler_frontend::ast::templates::template_types::Template;
    use crate::compiler_frontend::datatypes::Ownership;
    use crate::compiler_frontend::symbols::string_interning::StringTable;
    use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
    use std::rc::Rc;

    fn create_text_segment(
        text: &str,
        origin: TemplateSegmentOrigin,
        string_table: &mut StringTable,
    ) -> TemplateSegment {
        let interned = string_table.intern(text);
        TemplateSegment::new(
            Expression::string_slice(
                interned,
                SourceLocation::default(),
                Ownership::ImmutableOwned,
            ),
            origin,
        )
    }

    #[test]
    fn converts_simple_text_body_runs() {
        let mut string_table = StringTable::new();
        let mut content = TemplateContent::default();

        content.add_with_origin(
            create_text_segment("Hello ", TemplateSegmentOrigin::Body, &mut string_table)
                .expression,
            TemplateSegmentOrigin::Body,
        );

        let plan = TemplateRenderPlan::from_content(&content);
        assert_eq!(plan.pieces.len(), 1);

        match &plan.pieces[0] {
            RenderPiece::Text(piece) => {
                assert_eq!(string_table.resolve(piece.text), "Hello ");
            }
            _ => panic!("Expected text piece"),
        }
    }

    #[test]
    fn separates_head_content_from_body_text() {
        let mut string_table = StringTable::new();
        let mut content = TemplateContent::default();

        content.add_with_origin(
            create_text_segment("head_stuff", TemplateSegmentOrigin::Head, &mut string_table)
                .expression,
            TemplateSegmentOrigin::Head,
        );
        content.add_with_origin(
            create_text_segment("body_stuff", TemplateSegmentOrigin::Body, &mut string_table)
                .expression,
            TemplateSegmentOrigin::Body,
        );

        let plan = TemplateRenderPlan::from_content(&content);
        assert_eq!(plan.pieces.len(), 2);

        match &plan.pieces[0] {
            RenderPiece::HeadContent(piece) => {
                assert_eq!(string_table.resolve(piece.text), "head_stuff");
            }
            _ => panic!("Expected head content piece"),
        }

        match &plan.pieces[1] {
            RenderPiece::Text(piece) => {
                assert_eq!(string_table.resolve(piece.text), "body_stuff");
            }
            _ => panic!("Expected text piece"),
        }
    }

    #[test]
    fn identifies_child_template_output() {
        let mut string_table = StringTable::new();
        let mut content = TemplateContent::default();

        let child = Template::create_default(vec![]);
        let mut segment = create_text_segment(
            "child outputs",
            TemplateSegmentOrigin::Body,
            &mut string_table,
        );
        segment.is_child_template_output = true;
        segment.source_child_template = Some(Rc::new(child));

        content.atoms.push(TemplateAtom::Content(segment));

        let plan = TemplateRenderPlan::from_content(&content);
        assert_eq!(plan.pieces.len(), 1);

        match &plan.pieces[0] {
            RenderPiece::ChildTemplate(piece) => {
                match &piece.expression.kind {
                    crate::compiler_frontend::ast::expressions::expression::ExpressionKind::StringSlice(id) => {
                        assert_eq!(string_table.resolve(*id), "child outputs");
                    }
                    _ => panic!("Expected string slice expression in child template output"),
                }
            }
            _ => panic!("Expected child template piece"),
        }
    }

    #[test]
    fn preserves_slots() {
        let mut content = TemplateContent::default();
        content.atoms.push(TemplateAtom::Slot(SlotPlaceholder::new(
            SlotKey::Positional(0),
        )));

        let plan = TemplateRenderPlan::from_content(&content);
        assert_eq!(plan.pieces.len(), 1);

        match &plan.pieces[0] {
            RenderPiece::Slot(slot) => {
                assert_eq!(slot.key, SlotKey::Positional(0));
            }
            _ => panic!("Expected slot piece"),
        }
    }

    #[test]
    fn preserves_dynamic_expressions() {
        let mut content = TemplateContent::default();
        let expression = Expression::int(
            42,
            crate::compiler_frontend::tokenizer::tokens::SourceLocation::default(),
            Ownership::ImmutableOwned,
        );

        content.add_with_origin(expression.clone(), TemplateSegmentOrigin::Body);

        let plan = TemplateRenderPlan::from_content(&content);
        assert_eq!(plan.pieces.len(), 1);

        match &plan.pieces[0] {
            RenderPiece::DynamicExpression(piece) => match &piece.expression.kind {
                crate::compiler_frontend::ast::expressions::expression::ExpressionKind::Int(42) => {
                }
                _ => panic!("Expected dynamic integer expression"),
            },
            _ => panic!("Expected dynamic expression piece"),
        }
    }

    #[test]
    fn opaque_anchors_survive_structured_formatter() {
        use crate::compiler_frontend::ast::templates::template_render_plan::{
            FormatterAnchorId, FormatterInput, FormatterInputPiece, FormatterOpaqueKind,
            FormatterOpaquePiece, FormatterOutputPiece, FormatterTextPiece,
        };
        use crate::projects::html_project::styles::escape_html::escape_html_formatter;

        let mut string_table = StringTable::new();
        let hello = string_table.intern("<Hello> ");
        let world = string_table.intern(" &World");

        // Build input with text-anchor-text pattern.
        let input = FormatterInput {
            pieces: vec![
                FormatterInputPiece::Text(FormatterTextPiece {
                    text: hello,
                    location: SourceLocation::default(),
                }),
                FormatterInputPiece::Opaque(FormatterOpaquePiece {
                    id: FormatterAnchorId(42),
                    kind: FormatterOpaqueKind::ChildTemplate,
                }),
                FormatterInputPiece::Text(FormatterTextPiece {
                    text: world,
                    location: SourceLocation::default(),
                }),
            ],
        };

        // Run the escape_html formatter — text should be escaped, anchors preserved.
        let formatter = escape_html_formatter();
        let output = formatter
            .formatter
            .format(input, &mut string_table)
            .expect("escape_html formatter should succeed");

        assert_eq!(output.output.pieces.len(), 3);
        match &output.output.pieces[0] {
            FormatterOutputPiece::Text(t) => assert_eq!(t, "&lt;Hello&gt; "),
            _ => panic!("Expected escaped text"),
        }
        match &output.output.pieces[1] {
            FormatterOutputPiece::Opaque(anchor) => {
                assert_eq!(anchor.id, FormatterAnchorId(42));
                assert_eq!(anchor.kind, FormatterOpaqueKind::ChildTemplate);
            }
            _ => panic!("Expected opaque anchor"),
        }
        match &output.output.pieces[2] {
            FormatterOutputPiece::Text(t) => assert_eq!(t, " &amp;World"),
            _ => panic!("Expected escaped text"),
        }
    }
}
