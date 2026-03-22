#[cfg(test)]
mod render_plan_tests {
    use crate::compiler_frontend::ast::expressions::expression::Expression;
    use crate::compiler_frontend::ast::templates::template::{
        SlotKey, SlotPlaceholder, TemplateAtom, TemplateContent, TemplateSegment,
        TemplateSegmentOrigin, TemplateType,
    };
    use crate::compiler_frontend::ast::templates::template_render_plan::{
        RenderPiece, TemplateRenderPlan,
    };
    use crate::compiler_frontend::ast::templates::template_types::Template;
    use crate::compiler_frontend::datatypes::Ownership;
    use crate::compiler_frontend::string_interning::StringTable;
    use crate::compiler_frontend::tokenizer::tokens::TextLocation;

    fn create_text_segment(
        text: &str,
        origin: TemplateSegmentOrigin,
        string_table: &mut StringTable,
    ) -> TemplateSegment {
        let interned = string_table.intern(text);
        TemplateSegment::new(
            Expression::string_slice(interned, TextLocation::default(), Ownership::ImmutableOwned),
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
        segment.source_child_template = Some(Box::new(child));

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
            crate::compiler_frontend::tokenizer::tokens::TextLocation::default(),
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
    fn opaque_anchors_roundtrip_through_legacy_formatter() {
        use crate::compiler_frontend::ast::templates::template_render_plan::{
            FormatterAnchorId, FormatterInput, FormatterInputPiece, FormatterOutputPiece,
            FormatterTextPiece,
        };

        let mut string_table = StringTable::new();
        let hello = string_table.intern("Hello ");
        let world = string_table.intern(" World");

        // Build input with text-anchor-text pattern.
        let input = FormatterInput {
            pieces: vec![
                FormatterInputPiece::Text(FormatterTextPiece {
                    text: hello,
                    location: TextLocation::default(),
                }),
                FormatterInputPiece::Opaque(FormatterAnchorId(42)),
                FormatterInputPiece::Text(FormatterTextPiece {
                    text: world,
                    location: TextLocation::default(),
                }),
            ],
        };

        // Identity formatter — text passes through unchanged.
        let output = input.invoke_legacy_formatter(&string_table, |_| {});

        assert_eq!(output.pieces.len(), 3);
        match &output.pieces[0] {
            FormatterOutputPiece::Text(t) => assert_eq!(t, "Hello "),
            _ => panic!("Expected text"),
        }
        match &output.pieces[1] {
            FormatterOutputPiece::Opaque(id) => assert_eq!(*id, FormatterAnchorId(42)),
            _ => panic!("Expected opaque anchor"),
        }
        match &output.pieces[2] {
            FormatterOutputPiece::Text(t) => assert_eq!(t, " World"),
            _ => panic!("Expected text"),
        }
    }
}
