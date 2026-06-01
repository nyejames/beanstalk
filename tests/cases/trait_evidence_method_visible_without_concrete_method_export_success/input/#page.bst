import @widget/traits { DISPLAY_TEXT }
import @widget { PublicLabel, make_label }

value PublicLabel = make_label("hidden method")

[:trait_evidence_method_visible_without_concrete_method_export_success [value.display()]]
