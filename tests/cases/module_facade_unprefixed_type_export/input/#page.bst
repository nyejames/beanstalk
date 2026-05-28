import @sub { ExportedStruct }
import @sub { ExportedChoice }
import @sub { ExportedAlias }

instance = ExportedStruct("hello")
variant = ExportedChoice::VariantA
aliased_value ExportedAlias = "world"

io(instance.value)
